//! Skyward's own releases on GitHub: which is the newest, and the zip in
//! it for one phone, as the release workflow names each phone's.
//!
//! The OnePlus 15 builds come from the fork that carries them,
//! `liyongshun114514/skyward`, not from upstream: a phone must never be
//! offered a build made for another model or from another tree. The fork
//! tags a build `v<workspace version>-plk110.<build>`, such as
//! `v0.3.1-plk110.21`. Semver reads that as a pre-release of the
//! workspace's own 0.3.1, so `v0.3.1-plk110.21` comes after
//! `v0.3.1-plk110.20`, and the next upstream release, `v0.3.2`, comes
//! after both. The workspace version stays upstream's 0.3.1.
//!
//! The build a phone runs names the same version, from
//! `SKYWARD_BUILD_VERSION` (see `build.rs`), which a fork build sets to the
//! tag without its `v`. A build that names no fork version is one made
//! before the fork numbered its builds, or upstream's: it is held to be
//! older than the fork's first build of that version, so a phone that runs
//! the old 0.3.1 is offered `v0.3.1-plk110.21` and goes on from there.

use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;

/// The fork where the OnePlus 15 builds are published.
pub const OWNER: &str = "liyongshun114514";
pub const REPO: &str = "skyward";

/// Where GitHub answers which release of the fork is the newest.
pub const LATEST: &str = "https://api.github.com/repos/liyongshun114514/skyward/releases/latest";

/// Where GitHub lists the fork's releases, newest first. The list is asked
/// rather than [`LATEST`] so that a release for another phone is passed
/// over for the newest one that has this phone's zip.
const RELEASES: &str = "https://api.github.com/repos/liyongshun114514/skyward/releases?per_page=20";

/// The version of GitHub's API the answer is read as.
const API_VERSION: &str = "2022-11-28";

/// The newest release, as the build of Skyward that asked sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Newest {
    /// Its version, such as 0.3.1-plk110.21.
    pub version: String,
    /// It is newer than the build that asked.
    pub newer: bool,
    /// Its zip for the phone; none when it has none.
    pub build: Option<Build>,
}

/// A phone's zip in a release, which holds Skyward's image for the phone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Build {
    /// The zip's own name, such as
    /// skyward-v0.3.1-plk110.21-plk110-OS16.0.2.0.WOCNXM.zip.
    pub name: String,
    /// Where GitHub's API hands it out, which [`locate`] asks for where the
    /// file itself is.
    pub url: String,
    /// The release's own link to it on github.com.
    pub link: String,
    pub size: u64,
    /// Its SHA-256 as GitHub gives it, in hex.
    pub sha256: String,
}

impl Build {
    /// Where to download the zip from, in the order to try: the file's own
    /// short-lived address, where GitHub's API gave one, then the release's
    /// link on github.com, which some networks do not reach.
    pub fn sources(&self, located: Option<String>) -> Vec<String> {
        located.into_iter().chain([self.link.clone()]).collect()
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    url: String,
    name: String,
    size: u64,
    /// The digest GitHub took of the file as it was uploaded, such as
    /// `sha256:` and the SHA-256 in hex.
    digest: Option<String>,
    browser_download_url: String,
}

/// Why GitHub turned a question down, as it says.
#[derive(Deserialize)]
struct Message {
    message: String,
}

/// The version this build names itself. A fork build sets
/// `SKYWARD_BUILD_VERSION` to its tag without the `v`, such as
/// `0.3.1-plk110.21`; a build from the release tag picks it up through
/// `build.rs`. Where neither names one, the build is upstream's, whose
/// version the caller gives.
pub fn build_version(version: &str) -> &str {
    named(version, option_env!("SKYWARD_BUILD_VERSION"))
}

/// `named`, where it names something, else `version`.
fn named<'a>(version: &'a str, named: Option<&'a str>) -> &'a str {
    match named.map(str::trim) {
        Some(named) if !named.is_empty() => named,
        _ => version,
    }
}

/// The version a tag names, after the `v`.
fn tag_version(tag: &str) -> Result<semver::Version> {
    let version = tag.strip_prefix('v').with_context(|| format!("the release is tagged {tag}, which is not v and a version"))?;
    semver::Version::parse(version).with_context(|| format!("the release is tagged {tag}"))
}

/// The build a tag names after `plk110.`, such as 21 in
/// `v0.3.1-plk110.21`; none for a tag that names none.
fn tag_build(version: &semver::Version) -> Option<u32> {
    let mut parts = version.pre.as_str().split('.');
    match (parts.next(), parts.next()) {
        (Some("plk110"), Some(build)) => build.parse().ok(),
        _ => None,
    }
}

/// The newest release that `answer`, GitHub's answer at [`LATEST`], tells
/// of, and its zip for the phone `codename` names, as the build of version
/// `running` sees it.
///
/// This reads one release as it is, with no request of its own: [`ask`] is
/// what the recovery uses, which reads the list of releases.
pub fn newest(answer: &str, codename: &str, running: &str) -> Result<Newest> {
    let release: Release = serde_json::from_str(answer).context("read GitHub's answer")?;
    choose(&release, codename, running)
}

/// The newest of `release`, and its zip for `codename`, as the build of
/// version `running` sees it.
fn choose(release: &Release, codename: &str, running: &str) -> Result<Newest> {
    let tag = &release.tag_name;
    let version = tag_version(tag)?;
    if version.pre.as_str().starts_with("plk110.") {
        ensure!(tag_build(&version).is_some(), "the release is tagged {tag}, whose PLK110 build is not a number");
    }
    let this = semver::Version::parse(running).with_context(|| format!("this build's version is {running}"))?;
    Ok(Newest { version: version.to_string(), newer: newer_than(&version, &this), build: zip(release, tag, codename)? })
}

/// Whether `release` is newer than the build of version `running` that
/// asks.
///
/// Build numbers use semver ordering: build 21 follows build 20, and
/// a later upstream version follows both. The build identity is compiled
/// into the binary, so the same installed build is never offered again.
fn newer_than(release: &semver::Version, running: &semver::Version) -> bool {
    release > running
}

/// The zip in `release` for the phone `codename` names: an asset named
/// `skyward-<tag>-<codename>-` and ending in `.zip`, so that neither
/// another phone's zip nor one for a name that only starts with this
/// phone's is taken, with the SHA-256 GitHub gives it.
fn zip(release: &Release, tag: &str, codename: &str) -> Result<Option<Build>> {
    let start = format!("skyward-{tag}-{codename}-");
    let asset = release.assets.iter().find(|a| a.name.starts_with(&start) && a.name.ends_with(".zip"));
    let Some(a) = asset else { return Ok(None) };
    let sha256 = a.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")).with_context(|| format!("GitHub gives no SHA-256 of {}", a.name))?;
    ensure!(sha256.len() == 64 && sha256.bytes().all(|b| b.is_ascii_hexdigit()), "GitHub gives {} the SHA-256 {sha256}", a.name);
    Ok(Some(Build {
        sha256: sha256.to_lowercase(),
        name: a.name.clone(),
        url: a.url.clone(),
        link: a.browser_download_url.clone(),
        size: a.size,
    }))
}

/// Ask GitHub for the newest release of the fork that has a zip for the
/// phone `codename` names, as the build of version `running`.
///
/// A release for another phone only is passed over; where no release has
/// this phone's zip, the newest release is told of with no build, so that
/// the page says there is none rather than offering another phone's. Drafts
/// and releases marked pre-release are not offered at all: a fork build is
/// published as an ordinary release.
pub fn ask(agent: &ureq::Agent, codename: &str, running: &str) -> Result<Newest> {
    ask_at(agent, RELEASES, codename, build_version(running))
}

/// [`ask`], of the releases listed at `url`.
fn ask_at(agent: &ureq::Agent, url: &str, codename: &str, running: &str) -> Result<Newest> {
    let mut response = agent
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", API_VERSION)
        .header("User-Agent", format!("Skyward/{running}"))
        .call()
        .with_context(|| format!("ask {url}"))?;
    let status = response.status();
    let body = response.body_mut().read_to_string().with_context(|| format!("read the answer of {url}"))?;
    if !status.is_success() {
        bail!(refusal(status, &body));
    }
    let listed: Vec<Release> = serde_json::from_str(&body).with_context(|| format!("read the releases listed by {url}"))?;
    // Newest first, whichever order GitHub listed them in. A tag that is
    // not a version publishes no build, and is passed over.
    let mut releases: Vec<(semver::Version, Release)> = listed
        .into_iter()
        .filter(|r| !r.draft && !r.prerelease)
        .filter_map(|r| tag_version(&r.tag_name).ok().map(|v| (v, r)))
        .collect();
    releases.sort_by(|(a, _), (b, _)| b.cmp(a));
    let mut newest_any = None;
    for (_, release) in &releases {
        let candidate = choose(release, codename, running)?;
        // The newest release, whichever phone it is for: where it has no
        // zip for this one, the page says so.
        if newest_any.is_none() {
            newest_any = Some(candidate.clone());
        }
        // The newest build for this phone that is newer than the one
        // running; a release for another phone is passed over.
        if candidate.newer && candidate.build.is_some() {
            return Ok(candidate);
        }
    }
    newest_any.with_context(|| format!("{OWNER}/{REPO} has published no release"))
}

/// Where the file a build's `url` hands out is now, as GitHub's API sends
/// the build of version `running` that asks for it on: a short-lived
/// address of the file itself, away from github.com.
pub fn locate(agent: &ureq::Agent, url: &str, running: &str) -> Result<String> {
    let mut response = agent
        .get(url)
        .header("Accept", "application/octet-stream")
        .header("X-GitHub-Api-Version", API_VERSION)
        .header("User-Agent", format!("Skyward/{running}"))
        .config()
        .max_redirects(0)
        .build()
        .call()
        .with_context(|| format!("ask {url}"))?;
    let status = response.status();
    if status.is_redirection() {
        let location = response.headers().get(ureq::http::header::LOCATION).with_context(|| format!("GitHub answered {status}, and names no address"))?;
        return Ok(location.to_str().context("GitHub names an address that is not text")?.to_string());
    }
    ensure!(!status.is_success(), "GitHub sends the file itself in its answer, at no address of its own");
    let body = response.body_mut().read_to_string().with_context(|| format!("read the answer of {url}"))?;
    bail!(refusal(status, &body))
}

/// Why GitHub turned a question down, as it says, as when too many questions
/// came from the address that asked.
fn refusal(status: ureq::http::StatusCode, body: &str) -> String {
    match serde_json::from_str::<Message>(body) {
        Ok(m) => format!("GitHub answered {status}: {}", m.message),
        Err(_) => format!("GitHub answered {status}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What GitHub answered of the newest release on 2026-11-07, with the
    /// parts that do not matter here left out.
    const ANSWERED_013: &str = r#"{"url":"https://api.github.com/repos/kirklin/skyward/releases/437365758","tag_name":"v0.1.3","name":"v0.1.3","draft":false,"prerelease":false,"assets":[{"url":"https://api.github.com/repos/kirklin/skyward/releases/assets/719751468","id":719751468,"name":"SHA256SUMS","label":"","content_type":"application/octet-stream","state":"uploaded","size":2337,"digest":"sha256:052f9ca757baeb2eac78c8e546bcc54178f6bb9bf9e5bde53fa81b3f48448957","browser_download_url":"https://github.com/kirklin/skyward/releases/download/v0.1.3/SHA256SUMS"},{"url":"https://api.github.com/repos/kirklin/skyward/releases/assets/719751598","id":719751598,"name":"skyward-v0.1.3-ishtar-OS3.0.308.0.WMACNXM.zip","label":"Xiaomi 13 Ultra (ishtar)","content_type":"application/zip","state":"uploaded","size":42739608,"digest":"sha256:62b66f7d13a0eb6c9af3359b5ec6f9c223fd9a86b1f8abab3e9d2646cb2ce534","browser_download_url":"https://github.com/kirklin/skyward/releases/download/v0.1.3/skyward-v0.1.3-ishtar-OS3.0.308.0.WMACNXM.zip"},{"url":"https://api.github.com/repos/kirklin/skyward/releases/assets/719751747","id":719751747,"name":"skyward-v0.1.3-nezha-OS4.0.4.0.XPACNXM.zip","label":"Xiaomi 17 Ultra (nezha)","content_type":"application/zip","state":"uploaded","size":43145196,"digest":"sha256:9f6165079529418d8f6635beddfdd9abf9a70c89d69eb467a9922b0db69bfaf1","browser_download_url":"https://github.com/kirklin/skyward/releases/download/v0.1.3/skyward-v0.1.3-nezha-OS4.0.4.0.XPACNXM.zip"}]}"#;

    /// The fork's build 21 of the workspace's 0.3.1, as its release lists
    /// it: the zip the OnePlus 15 takes.
    const ANSWERED_PLK110_21: &str = r#"{"url":"https://api.github.com/repos/liyongshun114514/skyward/releases/1","tag_name":"v0.3.1-plk110.21","name":"v0.3.1-plk110.21","draft":false,"prerelease":false,"assets":[{"url":"https://api.github.com/repos/liyongshun114514/skyward/releases/assets/21","id":21,"name":"skyward-v0.3.1-plk110.21-plk110-OS16.0.2.0.WOCNXM.zip","label":"OnePlus 15 (plk110)","content_type":"application/zip","state":"uploaded","size":50331648,"digest":"sha256:abababababababababababababababababababababababababababababababab","browser_download_url":"https://github.com/liyongshun114514/skyward/releases/download/v0.3.1-plk110.21/skyward-v0.3.1-plk110.21-plk110-OS16.0.2.0.WOCNXM.zip"}]}"#;

    /// A release of the fork for the OnePlus 15 that has the version its
    /// tag names but no zip: what the phone is told of where no build was
    /// published.
    const ANSWERED_PLK110_NO_ZIP: &str = r#"{"tag_name":"v0.3.1-plk110.22","draft":false,"prerelease":false,"assets":[]}"#;

    #[test]
    fn only_the_fork_that_carries_the_one_plus_15_is_asked() {
        assert!(LATEST.starts_with("https://api.github.com/repos/liyongshun114514/skyward/"));
        assert!(!LATEST.contains("kirklin"), "upstream's releases are not the fork's");
        assert!(RELEASES.starts_with("https://api.github.com/repos/liyongshun114514/skyward/"));
    }

    #[test]
    fn a_fork_build_is_newer_than_the_build_it_follows() {
        let at = |running| newest(ANSWERED_PLK110_21, "plk110", running).unwrap();
        assert_eq!(at("0.3.1-plk110.20").version, "0.3.1-plk110.21");
        assert!(at("0.3.1-plk110.20").newer, "the build after 20");
        assert!(at("0.3.1-plk110.9").newer, "9 comes before 21, not after it");
        assert!(!at("0.3.1-plk110.21").newer, "the same build");
        assert!(!at("0.3.1-plk110.22").newer, "a build ahead of the release");
        assert!(!at("0.3.1").newer, "upstream's 0.3.1 is newer than a pre-release of it");
        assert!(!at("0.3.2").newer, "the next upstream release is newer than every fork build of 0.3.1");
    }

    #[test]
    fn a_build_names_its_fork_version_where_named_and_the_workspace_version_otherwise() {
        assert_eq!(named("0.3.1", Some("0.3.1-plk110.20")), "0.3.1-plk110.20");
        assert_eq!(named("0.3.1", Some("  0.3.1-plk110.20  ")), "0.3.1-plk110.20");
        assert_eq!(named("0.3.1", Some("   ")), "0.3.1");
        assert_eq!(named("0.3.1", None), "0.3.1");
    }

    #[test]
    fn a_build_number_is_read_from_the_tag() {
        let build = |tag: &str| tag_build(&tag_version(tag).unwrap());
        assert_eq!(build("v0.3.1-plk110.21"), Some(21));
        assert_eq!(build("v0.3.1-plk110.9"), Some(9));
        assert_eq!(build("v0.3.1"), None, "upstream's tag names no fork build");
        assert_eq!(build("v0.3.1-rc.1"), None, "another release's pre-release");
        let unnamed = ANSWERED_PLK110_21.replace("v0.3.1-plk110.21", "v0.3.1-plk110.beta");
        assert!(newest(&unnamed, "plk110", "0.3.1-plk110.20").is_err(), "a build that is not a number");
    }

    #[test]
    fn the_newest_release_gives_the_zip_for_the_phone() {
        let n = newest(ANSWERED_013, "ishtar", "0.1.2").unwrap();
        assert_eq!(n.version, "0.1.3");
        assert!(n.newer);
        assert_eq!(
            n.build,
            Some(Build {
                name: "skyward-v0.1.3-ishtar-OS3.0.308.0.WMACNXM.zip".into(),
                url: "https://api.github.com/repos/kirklin/skyward/releases/assets/719751598".into(),
                link: "https://github.com/kirklin/skyward/releases/download/v0.1.3/skyward-v0.1.3-ishtar-OS3.0.308.0.WMACNXM.zip".into(),
                size: 42739608,
                sha256: "62b66f7d13a0eb6c9af3359b5ec6f9c223fd9a86b1f8abab3e9d2646cb2ce534".into(),
            })
        );
        assert_eq!(newest(ANSWERED_013, "nezha", "0.1.2").unwrap().build.unwrap().size, 43145196);
    }

    #[test]
    fn a_zip_is_downloaded_from_where_github_sends_it_then_from_its_link() {
        let build = newest(ANSWERED_013, "ishtar", "0.1.2").unwrap().build.unwrap();
        let link = "https://github.com/kirklin/skyward/releases/download/v0.1.3/skyward-v0.1.3-ishtar-OS3.0.308.0.WMACNXM.zip";
        assert_eq!(build.sources(Some("https://release-assets.githubusercontent.com/a?sig=1".into())), ["https://release-assets.githubusercontent.com/a?sig=1", link]);
        assert_eq!(build.sources(None), [link], "where GitHub's API gave no address");
    }

    #[test]
    fn a_phone_is_found_by_its_whole_codename() {
        assert_eq!(newest(ANSWERED_013, "fuxi", "0.1.2").unwrap().build, None, "a phone the release has no zip for");
        assert_eq!(newest(ANSWERED_013, "ish", "0.1.2").unwrap().build, None, "the start of another phone's codename");
        assert_eq!(newest(ANSWERED_013, "SHA256SUMS", "0.1.2").unwrap().build, None);
        assert_eq!(newest(ANSWERED_PLK110_21, "plk1100", "0.3.1-plk110.20").unwrap().build, None, "a name that only starts with the phone's");
        assert_eq!(newest(ANSWERED_PLK110_21, "plk110", "0.3.1-plk110.20").unwrap().build.unwrap().size, 50331648);
    }

    #[test]
    fn a_release_without_the_phone_s_zip_offers_no_build_of_another_phone() {
        let n = newest(ANSWERED_PLK110_NO_ZIP, "plk110", "0.3.1-plk110.20").unwrap();
        assert_eq!(n.version, "0.3.1-plk110.22");
        assert!(n.newer);
        assert_eq!(n.build, None, "there is a newer release, but no package for this phone");
    }

    #[test]
    fn a_zip_needs_its_sha256_and_a_release_its_version() {
        let unsummed = ANSWERED_013.replace("\"digest\":\"sha256:62b66f7d13a0eb6c9af3359b5ec6f9c223fd9a86b1f8abab3e9d2646cb2ce534\"", "\"digest\":null");
        let e = newest(&unsummed, "ishtar", "0.1.2").unwrap_err();
        assert_eq!(e.to_string(), "GitHub gives no SHA-256 of skyward-v0.1.3-ishtar-OS3.0.308.0.WMACNXM.zip");
        assert!(newest(&unsummed, "nezha", "0.1.2").is_ok(), "another phone's zip has its own");
        let untagged = ANSWERED_013.replace("\"tag_name\":\"v0.1.3\"", "\"tag_name\":\"nightly\"");
        assert!(newest(&untagged, "ishtar", "0.1.2").is_err());
        assert!(newest("{}", "ishtar", "0.1.2").is_err());
    }

    /// A server on this computer that answers one request with `answer`,
    /// and gives the request it had.
    fn server(answer: String) -> (String, std::thread::JoinHandle<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/repos/kirklin/skyward/releases/assets/719751598", listener.local_addr().unwrap());
        let held = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut chunk = [0u8; 512];
            while !request.ends_with(b"\r\n\r\n") {
                let n = s.read(&mut chunk).unwrap();
                request.extend_from_slice(&chunk[..n]);
            }
            s.write_all(answer.as_bytes()).unwrap();
            String::from_utf8(request).unwrap()
        });
        (url, held)
    }

    #[test]
    fn github_s_api_sends_a_zip_on_to_where_the_file_is() {
        let agent = crate::http::agent(Default::default());
        let sent_on = "HTTP/1.1 302 Found\r\nLocation: https://release-assets.githubusercontent.com/a?sig=1\r\nContent-Length: 0\r\n\r\n";
        let (url, held) = server(sent_on.into());
        assert_eq!(locate(&agent, &url, "0.1.2").unwrap(), "https://release-assets.githubusercontent.com/a?sig=1", "the address, not followed");
        let request = held.join().unwrap().to_lowercase();
        assert!(request.contains("accept: application/octet-stream\r\n"), "asked for the file itself: {request}");
        assert!(request.contains("user-agent: skyward/0.1.2\r\n"), "{request}");
        let why = r#"{"message":"API rate limit exceeded for 203.0.113.7"}"#;
        let (url, _) = server(format!("HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{why}", why.len()));
        assert_eq!(locate(&agent, &url, "0.1.2").unwrap_err().to_string(), "GitHub answered 403 Forbidden: API rate limit exceeded for 203.0.113.7");
        let (url, _) = server("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nPK".into());
        assert!(locate(&agent, &url, "0.1.2").is_err(), "the file itself, with no address to download it from again");
    }

    /// A server on this computer that answers one request at `path` with
    /// `answer`, and gives the request it had.
    fn listing(path: &str, answer: &str) -> (String, std::thread::JoinHandle<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}{path}", listener.local_addr().unwrap());
        let answer = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{answer}", answer.len());
        let held = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut chunk = [0u8; 512];
            while !request.ends_with(b"\r\n\r\n") {
                let n = s.read(&mut chunk).unwrap();
                request.extend_from_slice(&chunk[..n]);
            }
            s.write_all(answer.as_bytes()).unwrap();
            String::from_utf8(request).unwrap()
        });
        (url, held)
    }

    #[test]
    fn the_releases_are_read_and_a_release_for_another_phone_is_passed_over() {
        let agent = crate::http::agent(Default::default());
        let ishtar = ANSWERED_013.replace("v0.1.3", "v0.3.1-plk110.22");
        let body = format!("[{ishtar},{}]", ANSWERED_PLK110_21);
        let (url, held) = listing("/releases?per_page=20", &body);
        let n = ask_at(&agent, &url, "plk110", "0.3.1-plk110.20").unwrap();
        assert_eq!(n.version, "0.3.1-plk110.21", "the newest release with a zip for the phone, not the newer one without");
        assert!(n.newer);
        assert_eq!(n.build.unwrap().name, "skyward-v0.3.1-plk110.21-plk110-OS16.0.2.0.WOCNXM.zip");
        let request = held.join().unwrap();
        assert!(request.starts_with("GET /releases?per_page=20 "), "{request}");
        assert!(request.to_lowercase().contains("accept: application/vnd.github+json\r\n"), "{request}");
    }

    #[test]
    fn a_fork_with_no_release_says_so() {
        let agent = crate::http::agent(Default::default());
        let (url, _) = listing("/releases?per_page=20", "[]");
        let e = ask_at(&agent, &url, "plk110", "0.3.1-plk110.20").unwrap_err();
        assert_eq!(e.to_string(), "liyongshun114514/skyward has published no release");
        let pre = format!("[{}]", ANSWERED_PLK110_21.replace("\"prerelease\":false", "\"prerelease\":true"));
        let (url, _) = listing("/releases?per_page=20", &pre);
        assert!(ask_at(&agent, &url, "plk110", "0.3.1-plk110.20").is_err(), "a release marked pre-release is not offered");
    }
}
