use crate::config::Remote;
use crate::error::*;
use reqwest_middleware::ClientWithMiddleware;
use serde::{
	Deserialize,
	Serialize,
};

use super::*;

/// GitLab REST API url.
const GITLAB_API_URL: &str = "https://gitlab.com/api/v4";

const GITLAB_API_PATH: &str = "/api/v4";

/// Representation of a single GitLab Project.
///
/// <https://docs.gitlab.com/ee/api/projects.html#get-single-project>
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitLabProject {
	/// GitLab id for project
	pub id: i64,
}

impl RemoteEntry for GitLabProject {
	fn url(_project_id: i64, api_url: &Url, remote: &Remote, _page: i32) -> String {
		format!("{}/projects/{}%2F{}", api_url, remote.owner, remote.repo)
	}

	fn buffer_size() -> usize {
		1
	}
}

/// Representation of a single commit.
///
/// <https://docs.gitlab.com/ee/api/commits.html>
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitLabCommit {
	/// Sha
	pub id:                    String,
	/// Author
	pub author_name:           String,
	pub(crate) committed_date: String,
}

impl From<GitLabCommit> for RemoteCommit {
	fn from(value: GitLabCommit) -> Self {
		Self {
			id:       value.id,
			username: Some(value.author_name),
		}
	}
}

impl RemoteEntry for GitLabCommit {
	fn url(id: i64, api_url: &Url, _remote: &Remote, page: i32) -> String {
		let commit_page = page + 1;
		format!(
			"{}/projects/{}/repository/commits?per_page={MAX_PAGE_SIZE}&\
			 page={commit_page}",
			api_url, id
		)
	}
	fn buffer_size() -> usize {
		10
	}
}

/// Representation of a single pull request.
///
/// <https://docs.gitlab.com/ee/api/merge_requests.html>
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitLabMergeRequest {
	/// Numeric project-wide ID
	pub iid:              i64,
	/// Title
	pub title:            String,
	/// Merge Commit Sha
	pub merge_commit_sha: Option<String>,
	/// Labels
	pub labels:           Vec<String>,
}

impl From<GitLabMergeRequest> for RemotePullRequest {
	fn from(value: GitLabMergeRequest) -> Self {
		Self {
			number:       value.iid,
			title:        Some(value.title),
			labels:       value.labels,
			merge_commit: value.merge_commit_sha,
		}
	}
}

impl RemoteEntry for GitLabMergeRequest {
	fn url(id: i64, api_url: &Url, _remote: &Remote, page: i32) -> String {
		format!(
			"{}/projects/{}/merge_requests?per_page={MAX_PAGE_SIZE}&page={page}&\
			 state=merged",
			api_url, id
		)
	}

	fn buffer_size() -> usize {
		5
	}
}

/// HTTP client for handling GitLab REST API requests.
#[derive(Debug, Clone)]
pub struct GitLabClient {
	/// Remote.
	remote:     Remote,
	/// GitLab API Url
	api_url:    Url,
	/// GitLab project ID
	project_id: i64,
	/// HTTP client.
	client:     ClientWithMiddleware,
}

/// Constructs a GitLab client from the remote configuration.
impl TryFrom<Remote> for GitLabClient {
	type Error = Error;
	fn try_from(remote: Remote) -> Result<Self> {
		Ok(Self {
			client: create_remote_client(&remote, "application/json")?,
			api_url: remote
				.url
				.as_ref()
				.filter(|url| url.domain() != Some("github.com"))
				.map(|url| {
					// GitHub Enterprise Server API URL
					let mut new_url = url.clone();
					new_url.set_path(GITLAB_API_PATH);
					new_url
				})
				.unwrap_or_else(|| Url::parse(GITLAB_API_URL).expect("invalid url")),
			project_id: 0,
			remote,
		})
	}
}

impl RemoteClientInternal for GitLabClient {
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
impl RemoteClient for GitLabClient {
	async fn init(&mut self) -> Result<()> {
		let project = self.get_entry::<GitLabProject>(0, 0).await?;
		self.project_id = project.id;
		Ok(())
	}

	async fn get_commits(&self) -> Result<Vec<RemoteCommit>> {
		Ok(self
			.fetch::<GitLabCommit>(self.project_id)
			.await?
			.into_iter()
			.map(RemoteCommit::from)
			.collect())
	}

	async fn get_pull_requests(&self) -> Result<Vec<RemotePullRequest>> {
		Ok(self
			.fetch::<GitLabMergeRequest>(self.project_id)
			.await?
			.into_iter()
			.map(RemotePullRequest::from)
			.collect())
	}
}
