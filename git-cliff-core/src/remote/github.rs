use crate::config::Remote;
use crate::error::*;
use reqwest_middleware::ClientWithMiddleware;
use serde::{
	Deserialize,
	Serialize,
};
use url::Url;

use super::*;

/// GitHub REST API url.
const GITHUB_API_URL: &str = "https://api.github.com";

const GHES_API_PATH: &str = "/api/v3";

/// Representation of a single commit.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitHubCommit {
	/// SHA.
	pub sha:    String,
	/// Author of the commit.
	pub author: Option<GitHubCommitAuthor>,
}

/// Author of the commit.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitHubCommitAuthor {
	/// Username.
	pub login: Option<String>,
}

/// Label of the pull request.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestLabel {
	/// Name of the label.
	pub name: String,
}

/// Representation of a single pull request.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitHubPullRequest {
	/// Pull request number.
	pub number:           i64,
	/// Pull request title.
	pub title:            Option<String>,
	/// SHA of the merge commit.
	pub merge_commit_sha: Option<String>,
	/// Labels of the pull request.
	pub labels:           Vec<PullRequestLabel>,
}

impl RemoteEntry for GitHubCommit {
	fn url(_project_id: i64, api_url: &Url, remote: &Remote, page: i32) -> String {
		format!(
			"{}/repos/{}/{}/commits?per_page={MAX_PAGE_SIZE}&page={page}",
			api_url, remote.owner, remote.repo
		)
	}

	fn buffer_size() -> usize {
		10
	}
}

impl RemoteEntry for GitHubPullRequest {
	fn url(_project_id: i64, api_url: &Url, remote: &Remote, page: i32) -> String {
		format!(
			"{}/repos/{}/{}/pulls?per_page={MAX_PAGE_SIZE}&page={page}&state=closed",
			api_url, remote.owner, remote.repo
		)
	}

	fn buffer_size() -> usize {
		5
	}
}

impl From<GitHubCommit> for RemoteCommit {
	fn from(value: GitHubCommit) -> Self {
		Self {
			id:       value.sha,
			username: value.author.and_then(|a| a.login),
		}
	}
}

impl From<GitHubPullRequest> for RemotePullRequest {
	fn from(value: GitHubPullRequest) -> Self {
		Self {
			number:       value.number,
			title:        value.title,
			labels:       value.labels.into_iter().map(|l| l.name).collect(),
			merge_commit: value.merge_commit_sha,
		}
	}
}

/// HTTP client for handling GitHub REST API requests.
#[derive(Debug, Clone)]
pub struct GitHubClient {
	/// Remote.
	remote:  Remote,
	/// GitHub API Url
	api_url: Url,
	/// HTTP client.
	client:  ClientWithMiddleware,
}

/// Constructs a GitHub client from the remote configuration.
impl TryFrom<Remote> for GitHubClient {
	type Error = Error;
	fn try_from(remote: Remote) -> Result<Self> {
		Ok(Self {
			client: create_remote_client(&remote, "application/vnd.github+json")?,
			api_url: remote
				.url
				.as_ref()
				.filter(|url| url.domain() != Some("github.com"))
				.map(|url| {
					// GitHub Enterprise Server API URL
					let mut new_url = url.clone();
					new_url.set_path(GHES_API_PATH);
					new_url
				})
				.unwrap_or_else(|| Url::parse(GITHUB_API_URL).expect("invalid url")),
			remote,
		})
	}
}

impl RemoteClientInternal for GitHubClient {
	fn api_url(&self) -> &Url {
		&self.api_url
	}

	fn remote(&self) -> Remote {
		self.remote.clone()
	}

	fn client(&self) -> ClientWithMiddleware {
		self.client.clone()
	}
}

#[async_trait]
impl RemoteClient for GitHubClient {
	async fn get_commits(&self) -> Result<Vec<RemoteCommit>> {
		Ok(self
			.fetch::<GitHubCommit>(0)
			.await?
			.into_iter()
			.map(RemoteCommit::from)
			.collect())
	}

	async fn get_pull_requests(&self) -> Result<Vec<RemotePullRequest>> {
		Ok(self
			.fetch::<GitHubPullRequest>(0)
			.await?
			.into_iter()
			.map(RemotePullRequest::from)
			.collect())
	}
}
