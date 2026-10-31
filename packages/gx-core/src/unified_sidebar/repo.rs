//! Which repository a project is, so the same checkout on two machines reads as one project.

/// A project as the Unified sidebar names and groups it.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepoIdentity {
    /// What two projects share when they are the same repository: the normalized origin
    /// (`github.com/owner/repo`), else `dir:` and the checkout's folder name.
    pub key: String,
    /// What a row and the filter show: the repository's name as its origin spells it, else the
    /// folder name.
    pub label: String,
}

/// The repository a project is.
///
/// CDXC:Sidebar 2026-10-30 DECISION:
/// User: "we should make the same repo on different machines somehow refer to the same project when we're in unified (repo name is used not the project name basically)". The Unified sidebar names a project by its git origin, which every gxserver already probes and publishes as `gitRemoteOriginUrl` (server/src/project_git_remote.rs publishes it raw, so normalizing is the client's job), and falls back to the checkout's folder name when there is no origin. A worktree project shares its parent's origin; without one it takes the parent's folder.
pub fn repo_identity(
    origin_url: Option<&str>,
    path: &str,
    parent_path: Option<&str>,
    fallback_title: &str,
) -> RepoIdentity {
    if let Some((key, label)) = origin_url.and_then(normalize_git_remote) {
        return RepoIdentity { key, label };
    }
    let folder = folder_name(parent_path.unwrap_or(path))
        .or_else(|| folder_name(path))
        .unwrap_or_else(|| fallback_title.trim().to_string());
    RepoIdentity {
        key: format!("dir:{}", folder.to_lowercase()),
        label: folder,
    }
}

/// `host/owner/repo` (lowercase) and the repository's name, for every form git prints an origin
/// in: `https://host/owner/repo(.git)`, `ssh://user@host:port/owner/repo`, `git@host:owner/repo`
/// and a local path. `None` for an empty value.
pub fn normalize_git_remote(url: &str) -> Option<(String, String)> {
    let url = url.trim().trim_end_matches('/');
    if url.is_empty() {
        return None;
    }
    let without_scheme = match url.split_once("://") {
        Some((_, rest)) => rest,
        None => url,
    };
    let (host, path) = match url.split_once("://") {
        // `scheme://[user@]host[:port]/path`
        Some(_) => {
            let (authority, path) = without_scheme
                .split_once('/')
                .unwrap_or((without_scheme, ""));
            let host = authority.rsplit('@').next().unwrap_or(authority);
            let host = host.split(':').next().unwrap_or(host);
            (host.to_string(), path.to_string())
        }
        None => match without_scheme.split_once(':') {
            // scp form `[user@]host:path`, but not a Windows drive (`C:\…`).
            Some((authority, path)) if authority.len() > 1 && !authority.contains('/') => {
                let host = authority.rsplit('@').next().unwrap_or(authority);
                (host.to_string(), path.to_string())
            }
            _ => (String::new(), without_scheme.to_string()),
        },
    };
    let path = path
        .trim_matches('/')
        .trim_end_matches(".git")
        .trim_end_matches('/')
        .replace('\\', "/");
    let label = path.rsplit('/').find(|part| !part.is_empty())?.to_string();
    let key = if host.is_empty() {
        path.to_lowercase()
    } else {
        format!("{}/{}", host.to_lowercase(), path.to_lowercase())
    };
    Some((key, label))
}

fn folder_name(path: &str) -> Option<String> {
    path.trim()
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .map(str::to_string)
}
