use crate::commit::Commit;
use crate::config::Bump;
use crate::error::Result;
#[cfg(feature = "remote")]
use crate::remote::{
	RemoteCommit,
	RemoteContributor,
	RemotePullRequest,
	RemoteReleaseMetadata,
};
use next_version::VersionUpdater;
use semver::Version;
use serde::{
	Deserialize,
	Serialize,
};

/// Representation of a release.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release<'a> {
	/// Release version, git tag.
	pub version:   Option<String>,
	/// Commits made for the release.
	pub commits:   Vec<Commit<'a>>,
	/// Commit ID of the tag.
	#[serde(rename = "commit_id")]
	pub commit_id: Option<String>,
	/// Timestamp of the release in seconds, from epoch.
	pub timestamp: i64,
	/// Previous release.
	pub previous:  Option<Box<Release<'a>>>,
	/// Contributors.
	#[cfg(feature = "github")]
	pub github:    RemoteReleaseMetadata,
	/// Contributors.
	#[cfg(feature = "gitlab")]
	pub gitlab:    RemoteReleaseMetadata,
	/// Contributors.
	#[cfg(feature = "gitea")]
	pub gitea:     RemoteReleaseMetadata,
	/// Contributors.
	#[cfg(feature = "bitbucket")]
	pub bitbucket: RemoteReleaseMetadata,
}

#[cfg(feature = "github")]
crate::update_release_metadata!(github, update_github_metadata);

#[cfg(feature = "gitlab")]
crate::update_release_metadata!(gitlab, update_gitlab_metadata);

#[cfg(feature = "gitea")]
crate::update_release_metadata!(gitea, update_gitea_metadata);

#[cfg(feature = "bitbucket")]
crate::update_release_metadata!(bitbucket, update_bitbucket_metadata);

impl<'a> Release<'a> {
	/// Calculates the next version based on the commits.
	///
	/// It uses the default bump version configuration to calculate the next
	/// version.
	pub fn calculate_next_version(&self) -> Result<String> {
		self.calculate_next_version_with_config(&Bump::default())
	}

	/// Calculates the next version based on the commits.
	///
	/// It uses the given bump version configuration to calculate the next
	/// version.
	pub(super) fn calculate_next_version_with_config(
		&self,
		config: &Bump,
	) -> Result<String> {
		match self
			.previous
			.as_ref()
			.and_then(|release| release.version.clone())
		{
			Some(version) => {
				let mut semver = Version::parse(&version);
				let mut prefix = None;
				if semver.is_err() && version.split('.').count() >= 2 {
					let mut found_numeric = false;
					for (i, c) in version.chars().enumerate() {
						if c.is_numeric() && !found_numeric {
							found_numeric = true;
							let version_prefix = version[..i].to_string();
							let remaining = version[i..].to_string();
							let version = Version::parse(&remaining);
							if version.is_ok() {
								semver = version;
								prefix = Some(version_prefix);
								break;
							}
						} else if !c.is_numeric() && found_numeric {
							found_numeric = false;
						}
					}
				}
				let next_version = VersionUpdater::new()
					.with_features_always_increment_minor(
						config.features_always_bump_minor.unwrap_or(true),
					)
					.with_breaking_always_increment_major(
						config.breaking_always_bump_major.unwrap_or(true),
					)
					.increment(
						&semver?,
						self.commits
							.iter()
							.map(|commit| commit.message.trim_end().to_string())
							.collect::<Vec<String>>(),
					)
					.to_string();
				if let Some(prefix) = prefix {
					Ok(format!("{prefix}{next_version}"))
				} else {
					Ok(next_version)
				}
			}
			None => {
				warn!("No releases found, using 0.1.0 as the next version.");
				Ok(String::from("0.1.0"))
			}
		}
	}
}

/// Representation of a list of releases.
#[derive(Serialize)]
pub struct Releases<'a> {
	/// Releases.
	pub releases: &'a Vec<Release<'a>>,
}

impl<'a> Releases<'a> {
	/// Returns the list of releases as JSON.
	pub fn as_json(&self) -> Result<String> {
		Ok(serde_json::to_string(self.releases)?)
	}
}

#[cfg(test)]
mod test {
	use super::*;
	#[test]
	fn bump_version() -> Result<()> {
		fn build_release<'a>(version: &str, commits: &'a [&str]) -> Release<'a> {
			Release {
				version: None,
				commits: commits
					.iter()
					.map(|v| Commit::from(v.to_string()))
					.collect(),
				commit_id: None,
				timestamp: 0,
				previous: Some(Box::new(Release {
					version: Some(String::from(version)),
					..Default::default()
				})),
				#[cfg(feature = "github")]
				github: crate::remote::RemoteReleaseMetadata {
					contributors: vec![],
				},
				#[cfg(feature = "gitlab")]
				gitlab: crate::remote::RemoteReleaseMetadata {
					contributors: vec![],
				},
				#[cfg(feature = "gitea")]
				gitea: crate::remote::RemoteReleaseMetadata {
					contributors: vec![],
				},
				#[cfg(feature = "bitbucket")]
				bitbucket: crate::remote::RemoteReleaseMetadata {
					contributors: vec![],
				},
			}
		}

		let test_shared = [
			("1.0.0", "1.1.0", vec!["feat: add xyz", "fix: fix xyz"]),
			("1.0.0", "1.0.1", vec!["fix: add xyz", "fix: aaaaaa"]),
			("1.0.0", "2.0.0", vec!["feat!: add xyz", "feat: zzz"]),
			("1.0.0", "2.0.0", vec!["feat!: add xyz\n", "feat: zzz\n"]),
			("2.0.0", "2.0.1", vec!["fix: something"]),
			("foo/1.0.0", "foo/1.1.0", vec![
				"feat: add xyz",
				"fix: fix xyz",
			]),
			("bar/1.0.0", "bar/2.0.0", vec![
				"fix: add xyz",
				"fix!: aaaaaa",
			]),
			("zzz-123/test/1.0.0", "zzz-123/test/1.0.1", vec![
				"fix: aaaaaa",
			]),
			("v100.0.0", "v101.0.0", vec!["feat!: something"]),
			("v1.0.0-alpha.1", "v1.0.0-alpha.2", vec!["fix: minor"]),
			("testing/v1.0.0-beta.1", "testing/v1.0.0-beta.2", vec![
				"feat: nice",
			]),
			("tauri-v1.5.4", "tauri-v1.6.0", vec!["feat: something"]),
			(
				"rocket/rocket-v4.0.0-rc.1",
				"rocket/rocket-v4.0.0-rc.2",
				vec!["chore!: wow"],
			),
			(
				"aaa#/@#$@9384!#%^#@#@!#!239432413-idk-9999.2200.5932-alpha.419",
				"aaa#/@#$@9384!#%^#@#@!#!239432413-idk-9999.2200.5932-alpha.420",
				vec!["feat: damn this is working"],
			),
		];

		for (version, expected_version, commits) in test_shared.iter().chain(
			[
				("0.0.1", "0.0.2", vec!["fix: fix xyz"]),
				("0.0.1", "0.1.0", vec!["feat: add xyz", "fix: fix xyz"]),
				("0.0.1", "1.0.0", vec!["feat!: add xyz", "feat: zzz"]),
				("0.1.0", "0.1.1", vec!["fix: fix xyz"]),
				("0.1.0", "0.2.0", vec!["feat: add xyz", "fix: fix xyz"]),
				("0.1.0", "1.0.0", vec!["feat!: add xyz", "feat: zzz"]),
			]
			.iter(),
		) {
			let release = build_release(version, commits);
			let next_version = release.calculate_next_version()?;
			assert_eq!(expected_version, &next_version);
			let next_version =
				release.calculate_next_version_with_config(&Bump::default())?;
			assert_eq!(expected_version, &next_version);
		}

		for (version, expected_version, commits) in test_shared.iter().chain(
			[
				("0.0.1", "0.0.2", vec!["fix: fix xyz"]),
				("0.0.1", "0.0.2", vec!["feat: add xyz", "fix: fix xyz"]),
				("0.0.1", "0.0.2", vec!["feat!: add xyz", "feat: zzz"]),
				("0.1.0", "0.1.1", vec!["fix: fix xyz"]),
				("0.1.0", "0.1.1", vec!["feat: add xyz", "fix: fix xyz"]),
				("0.1.0", "0.2.0", vec!["feat!: add xyz", "feat: zzz"]),
			]
			.iter(),
		) {
			let release = build_release(version, commits);
			let next_version =
				release.calculate_next_version_with_config(&Bump {
					features_always_bump_minor: Some(false),
					breaking_always_bump_major: Some(false),
				})?;
			assert_eq!(expected_version, &next_version);
		}

		for (version, expected_version, commits) in test_shared.iter().chain(
			[
				("0.0.1", "0.0.2", vec!["fix: fix xyz"]),
				("0.0.1", "0.1.0", vec!["feat: add xyz", "fix: fix xyz"]),
				("0.0.1", "0.1.0", vec!["feat!: add xyz", "feat: zzz"]),
				("0.1.0", "0.1.1", vec!["fix: fix xyz"]),
				("0.1.0", "0.2.0", vec!["feat: add xyz", "fix: fix xyz"]),
				("0.1.0", "0.2.0", vec!["feat!: add xyz", "feat: zzz"]),
			]
			.iter(),
		) {
			let release = build_release(version, commits);
			let next_version =
				release.calculate_next_version_with_config(&Bump {
					features_always_bump_minor: Some(true),
					breaking_always_bump_major: Some(false),
				})?;
			assert_eq!(expected_version, &next_version);
		}

		for (version, expected_version, commits) in test_shared.iter().chain(
			[
				("0.0.1", "0.0.2", vec!["fix: fix xyz"]),
				("0.0.1", "0.0.2", vec!["feat: add xyz", "fix: fix xyz"]),
				("0.0.1", "1.0.0", vec!["feat!: add xyz", "feat: zzz"]),
				("0.1.0", "0.1.1", vec!["fix: fix xyz"]),
				("0.1.0", "0.1.1", vec!["feat: add xyz", "fix: fix xyz"]),
				("0.1.0", "1.0.0", vec!["feat!: add xyz", "feat: zzz"]),
			]
			.iter(),
		) {
			let release = build_release(version, commits);
			let next_version =
				release.calculate_next_version_with_config(&Bump {
					features_always_bump_minor: Some(false),
					breaking_always_bump_major: Some(true),
				})?;
			assert_eq!(expected_version, &next_version);
		}

		let empty_release = Release {
			previous: Some(Box::new(Release {
				version: None,
				..Default::default()
			})),
			..Default::default()
		};
		assert_eq!("0.1.0", empty_release.calculate_next_version()?);
		for (features_always_bump_minor, breaking_always_bump_major) in
			[(true, true), (true, false), (false, true), (false, false)]
		{
			assert_eq!(
				"0.1.0",
				empty_release.calculate_next_version_with_config(&Bump {
					features_always_bump_minor: Some(features_always_bump_minor),
					breaking_always_bump_major: Some(breaking_always_bump_major),
				})?
			);
		}
		Ok(())
	}

	#[cfg(feature = "github")]
	#[test]
	fn update_github_metadata() -> Result<()> {
		use crate::remote::github::{
			GitHubCommit,
			GitHubCommitAuthor,
			GitHubPullRequest,
			PullRequestLabel,
		};

		let mut release = Release {
			version: None,
			commits: vec![
				Commit::from(String::from(
					"1d244937ee6ceb8e0314a4a201ba93a7a61f2071 add github \
					 integration",
				)),
				Commit::from(String::from(
					"21f6aa587fcb772de13f2fde0e92697c51f84162 fix github \
					 integration",
				)),
				Commit::from(String::from(
					"35d8c6b6329ecbcf131d7df02f93c3bbc5ba5973 update metadata",
				)),
				Commit::from(String::from(
					"4d3ffe4753b923f4d7807c490e650e6624a12074 do some stuff",
				)),
				Commit::from(String::from(
					"5a55e92e5a62dc5bf9872ffb2566959fad98bd05 alright",
				)),
				Commit::from(String::from(
					"6c34967147560ea09658776d4901709139b4ad66 should be fine",
				)),
			],
			commit_id: None,
			timestamp: 0,
			previous: Some(Box::new(Release {
				version: Some(String::from("1.0.0")),
				..Default::default()
			})),
			github: RemoteReleaseMetadata {
				contributors: vec![],
			},
			#[cfg(feature = "gitlab")]
			gitlab: RemoteReleaseMetadata {
				contributors: vec![],
			},
			#[cfg(feature = "gitea")]
			gitea: RemoteReleaseMetadata {
				contributors: vec![],
			},
			#[cfg(feature = "bitbucket")]
			bitbucket: RemoteReleaseMetadata {
				contributors: vec![],
			},
		};
		release.update_github_metadata(
			vec![
				GitHubCommit {
					sha:    String::from("1d244937ee6ceb8e0314a4a201ba93a7a61f2071"),
					author: Some(GitHubCommitAuthor {
						login: Some(String::from("orhun")),
					}),
				},
				GitHubCommit {
					sha:    String::from("21f6aa587fcb772de13f2fde0e92697c51f84162"),
					author: Some(GitHubCommitAuthor {
						login: Some(String::from("orhun")),
					}),
				},
				GitHubCommit {
					sha:    String::from("35d8c6b6329ecbcf131d7df02f93c3bbc5ba5973"),
					author: Some(GitHubCommitAuthor {
						login: Some(String::from("nuhro")),
					}),
				},
				GitHubCommit {
					sha:    String::from("4d3ffe4753b923f4d7807c490e650e6624a12074"),
					author: Some(GitHubCommitAuthor {
						login: Some(String::from("awesome_contributor")),
					}),
				},
				GitHubCommit {
					sha:    String::from("5a55e92e5a62dc5bf9872ffb2566959fad98bd05"),
					author: Some(GitHubCommitAuthor {
						login: Some(String::from("orhun")),
					}),
				},
				GitHubCommit {
					sha:    String::from("6c34967147560ea09658776d4901709139b4ad66"),
					author: Some(GitHubCommitAuthor {
						login: Some(String::from("someone")),
					}),
				},
				GitHubCommit {
					sha:    String::from("0c34967147560e809658776d4901709139b4ad68"),
					author: Some(GitHubCommitAuthor {
						login: Some(String::from("idk")),
					}),
				},
				GitHubCommit {
					sha:    String::from("kk34967147560e809658776d4901709139b4ad68"),
					author: None,
				},
				GitHubCommit {
					sha:    String::new(),
					author: None,
				},
			]
			.into_iter()
			.map(|v| Box::new(v) as Box<dyn RemoteCommit>)
			.collect(),
			vec![
				GitHubPullRequest {
					title:            Some(String::from("1")),
					number:           42,
					merge_commit_sha: Some(String::from(
						"1d244937ee6ceb8e0314a4a201ba93a7a61f2071",
					)),
					labels:           vec![PullRequestLabel {
						name: String::from("rust"),
					}],
				},
				GitHubPullRequest {
					title:            Some(String::from("2")),
					number:           66,
					merge_commit_sha: Some(String::from(
						"21f6aa587fcb772de13f2fde0e92697c51f84162",
					)),
					labels:           vec![PullRequestLabel {
						name: String::from("rust"),
					}],
				},
				GitHubPullRequest {
					title:            Some(String::from("3")),
					number:           53,
					merge_commit_sha: Some(String::from(
						"35d8c6b6329ecbcf131d7df02f93c3bbc5ba5973",
					)),
					labels:           vec![PullRequestLabel {
						name: String::from("deps"),
					}],
				},
				GitHubPullRequest {
					title:            Some(String::from("4")),
					number:           1000,
					merge_commit_sha: Some(String::from(
						"4d3ffe4753b923f4d7807c490e650e6624a12074",
					)),
					labels:           vec![PullRequestLabel {
						name: String::from("deps"),
					}],
				},
				GitHubPullRequest {
					title:            Some(String::from("5")),
					number:           999999,
					merge_commit_sha: Some(String::from(
						"5a55e92e5a62dc5bf9872ffb2566959fad98bd05",
					)),
					labels:           vec![PullRequestLabel {
						name: String::from("github"),
					}],
				},
			]
			.into_iter()
			.map(|v| Box::new(v) as Box<dyn RemotePullRequest>)
			.collect(),
		)?;
		let expected_commits = vec![
			Commit {
				id: String::from("1d244937ee6ceb8e0314a4a201ba93a7a61f2071"),
				message: String::from("add github integration"),
				github: RemoteContributor {
					username:      Some(String::from("orhun")),
					pr_title:      Some(String::from("1")),
					pr_number:     Some(42),
					pr_labels:     vec![String::from("rust")],
					is_first_time: false,
				},
				..Default::default()
			},
			Commit {
				id: String::from("21f6aa587fcb772de13f2fde0e92697c51f84162"),
				message: String::from("fix github integration"),
				github: RemoteContributor {
					username:      Some(String::from("orhun")),
					pr_title:      Some(String::from("2")),
					pr_number:     Some(66),
					pr_labels:     vec![String::from("rust")],
					is_first_time: false,
				},
				..Default::default()
			},
			Commit {
				id: String::from("35d8c6b6329ecbcf131d7df02f93c3bbc5ba5973"),
				message: String::from("update metadata"),
				github: RemoteContributor {
					username:      Some(String::from("nuhro")),
					pr_title:      Some(String::from("3")),
					pr_number:     Some(53),
					pr_labels:     vec![String::from("deps")],
					is_first_time: false,
				},
				..Default::default()
			},
			Commit {
				id: String::from("4d3ffe4753b923f4d7807c490e650e6624a12074"),
				message: String::from("do some stuff"),
				github: RemoteContributor {
					username:      Some(String::from("awesome_contributor")),
					pr_title:      Some(String::from("4")),
					pr_number:     Some(1000),
					pr_labels:     vec![String::from("deps")],
					is_first_time: false,
				},
				..Default::default()
			},
			Commit {
				id: String::from("5a55e92e5a62dc5bf9872ffb2566959fad98bd05"),
				message: String::from("alright"),
				github: RemoteContributor {
					username:      Some(String::from("orhun")),
					pr_title:      Some(String::from("5")),
					pr_number:     Some(999999),
					pr_labels:     vec![String::from("github")],
					is_first_time: false,
				},
				..Default::default()
			},
			Commit {
				id: String::from("6c34967147560ea09658776d4901709139b4ad66"),
				message: String::from("should be fine"),
				github: RemoteContributor {
					username:      Some(String::from("someone")),
					pr_title:      None,
					pr_number:     None,
					pr_labels:     vec![],
					is_first_time: false,
				},
				..Default::default()
			},
		];
		assert_eq!(expected_commits, release.commits);

		release
			.github
			.contributors
			.sort_by(|a, b| a.pr_number.cmp(&b.pr_number));

		let expected_metadata = RemoteReleaseMetadata {
			contributors: vec![
				RemoteContributor {
					username:      Some(String::from("someone")),
					pr_title:      None,
					pr_number:     None,
					pr_labels:     vec![],
					is_first_time: true,
				},
				RemoteContributor {
					username:      Some(String::from("orhun")),
					pr_title:      Some(String::from("1")),
					pr_number:     Some(42),
					pr_labels:     vec![String::from("rust")],
					is_first_time: true,
				},
				RemoteContributor {
					username:      Some(String::from("nuhro")),
					pr_title:      Some(String::from("3")),
					pr_number:     Some(53),
					pr_labels:     vec![String::from("deps")],
					is_first_time: true,
				},
				RemoteContributor {
					username:      Some(String::from("awesome_contributor")),
					pr_title:      Some(String::from("4")),
					pr_number:     Some(1000),
					pr_labels:     vec![String::from("deps")],
					is_first_time: true,
				},
			],
		};
		assert_eq!(expected_metadata, release.github);

		Ok(())
	}

	#[cfg(feature = "gitlab")]
	#[test]
	fn update_gitlab_metadata() -> Result<()> {
		use crate::remote::gitlab::{
			GitLabCommit,
			GitLabMergeRequest,
			GitLabUser,
		};

		let mut release = Release {
			version: None,
			commits: vec![
				Commit::from(String::from(
					"1d244937ee6ceb8e0314a4a201ba93a7a61f2071 add github \
					 integration",
				)),
				Commit::from(String::from(
					"21f6aa587fcb772de13f2fde0e92697c51f84162 fix github \
					 integration",
				)),
				Commit::from(String::from(
					"35d8c6b6329ecbcf131d7df02f93c3bbc5ba5973 update metadata",
				)),
				Commit::from(String::from(
					"4d3ffe4753b923f4d7807c490e650e6624a12074 do some stuff",
				)),
				Commit::from(String::from(
					"5a55e92e5a62dc5bf9872ffb2566959fad98bd05 alright",
				)),
				Commit::from(String::from(
					"6c34967147560ea09658776d4901709139b4ad66 should be fine",
				)),
			],
			commit_id: None,
			timestamp: 0,
			previous: Some(Box::new(Release {
				version: Some(String::from("1.0.0")),
				..Default::default()
			})),
			#[cfg(feature = "github")]
			github: RemoteReleaseMetadata {
				contributors: vec![],
			},
			gitlab: RemoteReleaseMetadata {
				contributors: vec![],
			},
			#[cfg(feature = "gitea")]
			gitea: RemoteReleaseMetadata {
				contributors: vec![],
			},
			#[cfg(feature = "bitbucket")]
			bitbucket: RemoteReleaseMetadata {
				contributors: vec![],
			},
		};
		release.update_gitlab_metadata(
			vec![
				GitLabCommit {
					id:              String::from(
						"1d244937ee6ceb8e0314a4a201ba93a7a61f2071",
					),
					author_name:     String::from("orhun"),
					short_id:        String::from(""),
					title:           String::from(""),
					author_email:    String::from(""),
					authored_date:   String::from(""),
					committer_name:  String::from(""),
					committer_email: String::from(""),
					committed_date:  String::from(""),
					created_at:      String::from(""),
					message:         String::from(""),
					parent_ids:      vec![],
					web_url:         String::from(""),
				},
				GitLabCommit {
					id:              String::from(
						"21f6aa587fcb772de13f2fde0e92697c51f84162",
					),
					author_name:     String::from("orhun"),
					short_id:        String::from(""),
					title:           String::from(""),
					author_email:    String::from(""),
					authored_date:   String::from(""),
					committer_name:  String::from(""),
					committer_email: String::from(""),
					committed_date:  String::from(""),
					created_at:      String::from(""),
					message:         String::from(""),
					parent_ids:      vec![],
					web_url:         String::from(""),
				},
				GitLabCommit {
					id:              String::from(
						"35d8c6b6329ecbcf131d7df02f93c3bbc5ba5973",
					),
					author_name:     String::from("nuhro"),
					short_id:        String::from(""),
					title:           String::from(""),
					author_email:    String::from(""),
					authored_date:   String::from(""),
					committer_name:  String::from(""),
					committer_email: String::from(""),
					committed_date:  String::from(""),
					created_at:      String::from(""),
					message:         String::from(""),
					parent_ids:      vec![],
					web_url:         String::from(""),
				},
				GitLabCommit {
					id:              String::from(
						"4d3ffe4753b923f4d7807c490e650e6624a12074",
					),
					author_name:     String::from("awesome_contributor"),
					short_id:        String::from(""),
					title:           String::from(""),
					author_email:    String::from(""),
					authored_date:   String::from(""),
					committer_name:  String::from(""),
					committer_email: String::from(""),
					committed_date:  String::from(""),
					created_at:      String::from(""),
					message:         String::from(""),
					parent_ids:      vec![],
					web_url:         String::from(""),
				},
				GitLabCommit {
					id:              String::from(
						"5a55e92e5a62dc5bf9872ffb2566959fad98bd05",
					),
					author_name:     String::from("orhun"),
					short_id:        String::from(""),
					title:           String::from(""),
					author_email:    String::from(""),
					authored_date:   String::from(""),
					committer_name:  String::from(""),
					committer_email: String::from(""),
					committed_date:  String::from(""),
					created_at:      String::from(""),
					message:         String::from(""),
					parent_ids:      vec![],
					web_url:         String::from(""),
				},
				GitLabCommit {
					id:              String::from(
						"6c34967147560ea09658776d4901709139b4ad66",
					),
					author_name:     String::from("someone"),
					short_id:        String::from(""),
					title:           String::from(""),
					author_email:    String::from(""),
					authored_date:   String::from(""),
					committer_name:  String::from(""),
					committer_email: String::from(""),
					committed_date:  String::from(""),
					created_at:      String::from(""),
					message:         String::from(""),
					parent_ids:      vec![],
					web_url:         String::from(""),
				},
				GitLabCommit {
					id:              String::from(
						"0c34967147560e809658776d4901709139b4ad68",
					),
					author_name:     String::from("idk"),
					short_id:        String::from(""),
					title:           String::from(""),
					author_email:    String::from(""),
					authored_date:   String::from(""),
					committer_name:  String::from(""),
					committer_email: String::from(""),
					committed_date:  String::from(""),
					created_at:      String::from(""),
					message:         String::from(""),
					parent_ids:      vec![],
					web_url:         String::from(""),
				},
				GitLabCommit {
					id:              String::from(
						"kk34967147560e809658776d4901709139b4ad68",
					),
					author_name:     String::from("orhun"),
					short_id:        String::from(""),
					title:           String::from(""),
					author_email:    String::from(""),
					authored_date:   String::from(""),
					committer_name:  String::from(""),
					committer_email: String::from(""),
					committed_date:  String::from(""),
					created_at:      String::from(""),
					message:         String::from(""),
					parent_ids:      vec![],
					web_url:         String::from(""),
				},
			]
			.into_iter()
			.map(|v| Box::new(v) as Box<dyn RemoteCommit>)
			.collect(),
			vec![Box::new(GitLabMergeRequest {
				title:             String::from("1"),
				merge_commit_sha:  Some(String::from(
					"1d244937ee6ceb8e0314a4a201ba93a7a61f2071",
				)),
				id:                1,
				iid:               1,
				project_id:        1,
				description:       String::from(""),
				state:             String::from(""),
				created_at:        String::from(""),
				author:            GitLabUser {
					id:         1,
					name:       String::from("42"),
					username:   String::from("42"),
					state:      String::from("42"),
					avatar_url: None,
					web_url:    String::from("42"),
				},
				sha:               String::from(
					"1d244937ee6ceb8e0314a4a201ba93a7a61f2071",
				),
				web_url:           String::from(""),
				squash_commit_sha: None,
				labels:            vec![String::from("rust")],
			})],
		)?;
		let expected_commits = vec![
			Commit {
				id: String::from("1d244937ee6ceb8e0314a4a201ba93a7a61f2071"),
				message: String::from("add github integration"),
				gitlab: RemoteContributor {
					username:      Some(String::from("orhun")),
					pr_title:      Some(String::from("1")),
					pr_number:     Some(1),
					pr_labels:     vec![String::from("rust")],
					is_first_time: false,
				},
				..Default::default()
			},
			Commit {
				id: String::from("21f6aa587fcb772de13f2fde0e92697c51f84162"),
				message: String::from("fix github integration"),
				gitlab: RemoteContributor {
					username:      Some(String::from("orhun")),
					pr_title:      None,
					pr_number:     None,
					pr_labels:     vec![],
					is_first_time: false,
				},
				..Default::default()
			},
			Commit {
				id: String::from("35d8c6b6329ecbcf131d7df02f93c3bbc5ba5973"),
				message: String::from("update metadata"),
				gitlab: RemoteContributor {
					username:      Some(String::from("nuhro")),
					pr_title:      None,
					pr_number:     None,
					pr_labels:     vec![],
					is_first_time: false,
				},
				..Default::default()
			},
			Commit {
				id: String::from("4d3ffe4753b923f4d7807c490e650e6624a12074"),
				message: String::from("do some stuff"),
				gitlab: RemoteContributor {
					username:      Some(String::from("awesome_contributor")),
					pr_title:      None,
					pr_number:     None,
					pr_labels:     vec![],
					is_first_time: false,
				},
				..Default::default()
			},
			Commit {
				id: String::from("5a55e92e5a62dc5bf9872ffb2566959fad98bd05"),
				message: String::from("alright"),
				gitlab: RemoteContributor {
					username:      Some(String::from("orhun")),
					pr_title:      None,
					pr_number:     None,
					pr_labels:     vec![],
					is_first_time: false,
				},
				..Default::default()
			},
			Commit {
				id: String::from("6c34967147560ea09658776d4901709139b4ad66"),
				message: String::from("should be fine"),
				gitlab: RemoteContributor {
					username:      Some(String::from("someone")),
					pr_title:      None,
					pr_number:     None,
					pr_labels:     vec![],
					is_first_time: false,
				},
				..Default::default()
			},
		];
		assert_eq!(expected_commits, release.commits);

		release
			.github
			.contributors
			.sort_by(|a, b| a.pr_number.cmp(&b.pr_number));

		let expected_metadata = RemoteReleaseMetadata {
			contributors: vec![
				RemoteContributor {
					username:      Some(String::from("orhun")),
					pr_title:      Some(String::from("1")),
					pr_number:     Some(1),
					pr_labels:     vec![String::from("rust")],
					is_first_time: false,
				},
				RemoteContributor {
					username:      Some(String::from("nuhro")),
					pr_title:      None,
					pr_number:     None,
					pr_labels:     vec![],
					is_first_time: true,
				},
				RemoteContributor {
					username:      Some(String::from("awesome_contributor")),
					pr_title:      None,
					pr_number:     None,
					pr_labels:     vec![],
					is_first_time: true,
				},
				RemoteContributor {
					username:      Some(String::from("someone")),
					pr_title:      None,
					pr_number:     None,
					pr_labels:     vec![],
					is_first_time: true,
				},
			],
		};
		assert_eq!(expected_metadata, release.gitlab);

		Ok(())
	}

	#[cfg(feature = "gitea")]
	#[test]
	fn update_gitea_metadata() -> Result<()> {
		use crate::remote::gitea::{
			GiteaCommit,
			GiteaCommitAuthor,
			GiteaPullRequest,
			PullRequestLabel,
		};

		let mut release = Release {
			version: None,
			commits: vec![
				Commit::from(String::from(
					"1d244937ee6ceb8e0314a4a201ba93a7a61f2071 add github \
					 integration",
				)),
				Commit::from(String::from(
					"21f6aa587fcb772de13f2fde0e92697c51f84162 fix github \
					 integration",
				)),
				Commit::from(String::from(
					"35d8c6b6329ecbcf131d7df02f93c3bbc5ba5973 update metadata",
				)),
				Commit::from(String::from(
					"4d3ffe4753b923f4d7807c490e650e6624a12074 do some stuff",
				)),
				Commit::from(String::from(
					"5a55e92e5a62dc5bf9872ffb2566959fad98bd05 alright",
				)),
				Commit::from(String::from(
					"6c34967147560ea09658776d4901709139b4ad66 should be fine",
				)),
			],
			commit_id: None,
			timestamp: 0,
			previous: Some(Box::new(Release {
				version: Some(String::from("1.0.0")),
				..Default::default()
			})),
			#[cfg(feature = "github")]
			github: RemoteReleaseMetadata {
				contributors: vec![],
			},
			#[cfg(feature = "gitlab")]
			gitlab: RemoteReleaseMetadata {
				contributors: vec![],
			},
			gitea: RemoteReleaseMetadata {
				contributors: vec![],
			},
			#[cfg(feature = "bitbucket")]
			bitbucket: RemoteReleaseMetadata {
				contributors: vec![],
			},
		};
		release.update_gitea_metadata(
			vec![
				GiteaCommit {
					sha:    String::from("1d244937ee6ceb8e0314a4a201ba93a7a61f2071"),
					author: Some(GiteaCommitAuthor {
						login: Some(String::from("orhun")),
					}),
				},
				GiteaCommit {
					sha:    String::from("21f6aa587fcb772de13f2fde0e92697c51f84162"),
					author: Some(GiteaCommitAuthor {
						login: Some(String::from("orhun")),
					}),
				},
				GiteaCommit {
					sha:    String::from("35d8c6b6329ecbcf131d7df02f93c3bbc5ba5973"),
					author: Some(GiteaCommitAuthor {
						login: Some(String::from("nuhro")),
					}),
				},
				GiteaCommit {
					sha:    String::from("4d3ffe4753b923f4d7807c490e650e6624a12074"),
					author: Some(GiteaCommitAuthor {
						login: Some(String::from("awesome_contributor")),
					}),
				},
				GiteaCommit {
					sha:    String::from("5a55e92e5a62dc5bf9872ffb2566959fad98bd05"),
					author: Some(GiteaCommitAuthor {
						login: Some(String::from("orhun")),
					}),
				},
				GiteaCommit {
					sha:    String::from("6c34967147560ea09658776d4901709139b4ad66"),
					author: Some(GiteaCommitAuthor {
						login: Some(String::from("someone")),
					}),
				},
				GiteaCommit {
					sha:    String::from("0c34967147560e809658776d4901709139b4ad68"),
					author: Some(GiteaCommitAuthor {
						login: Some(String::from("idk")),
					}),
				},
				GiteaCommit {
					sha:    String::from("kk34967147560e809658776d4901709139b4ad68"),
					author: None,
				},
				GiteaCommit {
					sha:    String::new(),
					author: None,
				},
			]
			.into_iter()
			.map(|v| Box::new(v) as Box<dyn RemoteCommit>)
			.collect(),
			vec![
				GiteaPullRequest {
					title:            Some(String::from("1")),
					number:           42,
					merge_commit_sha: Some(String::from(
						"1d244937ee6ceb8e0314a4a201ba93a7a61f2071",
					)),
					labels:           vec![PullRequestLabel {
						name: String::from("rust"),
					}],
				},
				GiteaPullRequest {
					title:            Some(String::from("2")),
					number:           66,
					merge_commit_sha: Some(String::from(
						"21f6aa587fcb772de13f2fde0e92697c51f84162",
					)),
					labels:           vec![PullRequestLabel {
						name: String::from("rust"),
					}],
				},
				GiteaPullRequest {
					title:            Some(String::from("3")),
					number:           53,
					merge_commit_sha: Some(String::from(
						"35d8c6b6329ecbcf131d7df02f93c3bbc5ba5973",
					)),
					labels:           vec![PullRequestLabel {
						name: String::from("deps"),
					}],
				},
				GiteaPullRequest {
					title:            Some(String::from("4")),
					number:           1000,
					merge_commit_sha: Some(String::from(
						"4d3ffe4753b923f4d7807c490e650e6624a12074",
					)),
					labels:           vec![PullRequestLabel {
						name: String::from("deps"),
					}],
				},
				GiteaPullRequest {
					title:            Some(String::from("5")),
					number:           999999,
					merge_commit_sha: Some(String::from(
						"5a55e92e5a62dc5bf9872ffb2566959fad98bd05",
					)),
					labels:           vec![PullRequestLabel {
						name: String::from("github"),
					}],
				},
			]
			.into_iter()
			.map(|v| Box::new(v) as Box<dyn RemotePullRequest>)
			.collect(),
		)?;
		let expected_commits = vec![
			Commit {
				id: String::from("1d244937ee6ceb8e0314a4a201ba93a7a61f2071"),
				message: String::from("add github integration"),
				github: RemoteContributor {
					username:      Some(String::from("orhun")),
					pr_title:      Some(String::from("1")),
					pr_number:     Some(42),
					pr_labels:     vec![String::from("rust")],
					is_first_time: false,
				},
				..Default::default()
			},
			Commit {
				id: String::from("21f6aa587fcb772de13f2fde0e92697c51f84162"),
				message: String::from("fix github integration"),
				github: RemoteContributor {
					username:      Some(String::from("orhun")),
					pr_title:      Some(String::from("2")),
					pr_number:     Some(66),
					pr_labels:     vec![String::from("rust")],
					is_first_time: false,
				},
				..Default::default()
			},
			Commit {
				id: String::from("35d8c6b6329ecbcf131d7df02f93c3bbc5ba5973"),
				message: String::from("update metadata"),
				github: RemoteContributor {
					username:      Some(String::from("nuhro")),
					pr_title:      Some(String::from("3")),
					pr_number:     Some(53),
					pr_labels:     vec![String::from("deps")],
					is_first_time: false,
				},
				..Default::default()
			},
			Commit {
				id: String::from("4d3ffe4753b923f4d7807c490e650e6624a12074"),
				message: String::from("do some stuff"),
				github: RemoteContributor {
					username:      Some(String::from("awesome_contributor")),
					pr_title:      Some(String::from("4")),
					pr_number:     Some(1000),
					pr_labels:     vec![String::from("deps")],
					is_first_time: false,
				},
				..Default::default()
			},
			Commit {
				id: String::from("5a55e92e5a62dc5bf9872ffb2566959fad98bd05"),
				message: String::from("alright"),
				github: RemoteContributor {
					username:      Some(String::from("orhun")),
					pr_title:      Some(String::from("5")),
					pr_number:     Some(999999),
					pr_labels:     vec![String::from("github")],
					is_first_time: false,
				},
				..Default::default()
			},
			Commit {
				id: String::from("6c34967147560ea09658776d4901709139b4ad66"),
				message: String::from("should be fine"),
				github: RemoteContributor {
					username:      Some(String::from("someone")),
					pr_title:      None,
					pr_number:     None,
					pr_labels:     vec![],
					is_first_time: false,
				},
				..Default::default()
			},
		];
		assert_eq!(expected_commits, release.commits);

		release
			.github
			.contributors
			.sort_by(|a, b| a.pr_number.cmp(&b.pr_number));

		let expected_metadata = RemoteReleaseMetadata {
			contributors: vec![
				RemoteContributor {
					username:      Some(String::from("someone")),
					pr_title:      None,
					pr_number:     None,
					pr_labels:     vec![],
					is_first_time: true,
				},
				RemoteContributor {
					username:      Some(String::from("orhun")),
					pr_title:      Some(String::from("1")),
					pr_number:     Some(42),
					pr_labels:     vec![String::from("rust")],
					is_first_time: true,
				},
				RemoteContributor {
					username:      Some(String::from("nuhro")),
					pr_title:      Some(String::from("3")),
					pr_number:     Some(53),
					pr_labels:     vec![String::from("deps")],
					is_first_time: true,
				},
				RemoteContributor {
					username:      Some(String::from("awesome_contributor")),
					pr_title:      Some(String::from("4")),
					pr_number:     Some(1000),
					pr_labels:     vec![String::from("deps")],
					is_first_time: true,
				},
			],
		};
		assert_eq!(expected_metadata, release.github);

		Ok(())
	}
}
