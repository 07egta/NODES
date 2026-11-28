//! Offline, data-only sharing envelope for existing .morphpart definitions.
//! Parsing or inspecting a package never runs its graph or loads external code.
//! This is a restricted format, not a general-purpose plugin sandbox.
use crate::cluster::{self, PartFile};
use crate::model::{Graph, NodeKind};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

pub const PACKAGE_FORMAT: &str = "nodes-cluster-package";
pub const PACKAGE_VERSION: u32 = 1;
pub const MAX_PACKAGE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_MANIFEST_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub version: String,
    pub title: String,
    pub author: String,
    pub description: String,
    pub license: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

fn metadata(value: &str, limit: usize, required: bool) -> bool {
    value.len() <= limit
        && (!required || !value.trim().is_empty())
        && !value
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
}

fn valid_id(id: &str) -> bool {
    id.len() <= 128
        && id.contains('.')
        && id.split('.').all(|segment| {
            !segment.is_empty()
                && segment.as_bytes()[0].is_ascii_lowercase()
                && segment
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
                && !segment.ends_with('-')
        })
}

// Stable SemVer subset: major.minor.patch only. Do not silently accept
// pre-release/build identifiers or version ranges as exact release identities.
fn stable_version(version: &str) -> bool {
    let parts: Vec<_> = version.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && (part.len() == 1 || !part.starts_with('0'))
                && part.bytes().all(|c| c.is_ascii_digit())
                && part.parse::<u64>().is_ok()
        })
}

impl Manifest {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.id) {
            return Err("パッケージ ID は author.name 形式の英小文字で指定してください。".into());
        }
        if self.version.len() > 64 || !stable_version(&self.version) {
            return Err("バージョンは major.minor.patch の安定版番号で指定してください。".into());
        }
        if !metadata(&self.title, 256, true)
            || !metadata(&self.author, 256, true)
            || !metadata(&self.description, 4096, false)
            || !metadata(&self.license, 256, true)
            || self.tags.len() > 32
            || self.tags.iter().any(|tag| !metadata(tag, 64, true))
        {
            return Err("パッケージの説明、作者、ライセンス、タグが正しくありません。".into());
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format: String,
    schema_version: u32,
    manifest: Manifest,
    // Retain exact UTF-8 part bytes, including whitespace; never rewrite unknown
    // node kinds into a different kind or silently drop them.
    part_json: String,
}

#[derive(Debug)]
pub struct Package {
    envelope: Envelope,
    part: PartFile,
}

#[derive(Debug, Serialize)]
pub struct PackageInfo<'a> {
    pub schema_version: u32,
    pub manifest: &'a Manifest,
    pub root: u64,
    pub root_name: &'a str,
    pub definition_count: usize,
    pub node_count: usize,
    pub part_bytes: usize,
    pub evaluation_performed: bool,
}

fn validate_part(bytes: &[u8]) -> Result<PartFile, String> {
    let part = cluster::parse_part(bytes)?;
    // Check EVERY definition, even unreachable and disabled ones. A nested
    // Cluster is a data-only reference validated by parse_part; all other nodes
    // must be in the reviewed Graph API policy and have no text/script payload.
    for def in &part.clusters {
        for node in &def.graph.nodes {
            if (node.kind != NodeKind::Cluster && !crate::graph_api::permitted(node.kind))
                || !node.script.is_empty()
            {
                return Err(format!(
                    "共有パッケージでは部品 {} のノード {} ({:?}) を許可していません。",
                    def.id, node.id, node.kind
                ));
            }
        }
    }
    Ok(part)
}

impl Package {
    pub fn from_part(manifest: Manifest, bytes: &[u8]) -> Result<Self, String> {
        manifest.validate()?;
        let part = validate_part(bytes)?;
        let part_json = std::str::from_utf8(bytes)
            .map_err(|_| "部品は UTF-8 JSON で指定してください。".to_string())?
            .to_owned();
        Ok(Self {
            envelope: Envelope {
                format: PACKAGE_FORMAT.into(),
                schema_version: PACKAGE_VERSION,
                manifest,
                part_json,
            },
            part,
        })
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_PACKAGE_BYTES {
            return Err("パッケージは最大 4 MiB です。".into());
        }
        let envelope: Envelope = serde_json::from_slice(bytes)
            .map_err(|e| format!("パッケージ JSON が正しくありません（{} 行目）。", e.line()))?;
        if envelope.format != PACKAGE_FORMAT || envelope.schema_version != PACKAGE_VERSION {
            return Err("未対応のパッケージ形式またはバージョンです。".into());
        }
        envelope.manifest.validate()?;
        let part = validate_part(envelope.part_json.as_bytes())?;
        Ok(Self { envelope, part })
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let mut bytes = serde_json::to_vec_pretty(&self.envelope)
            .map_err(|e| format!("パッケージを保存できません: {e}"))?;
        bytes.push(b'\n');
        if bytes.len() > MAX_PACKAGE_BYTES {
            return Err("パッケージは最大 4 MiB です。".into());
        }
        Ok(bytes)
    }

    pub fn info(&self) -> PackageInfo<'_> {
        PackageInfo {
            schema_version: PACKAGE_VERSION,
            manifest: &self.envelope.manifest,
            root: self.part.root,
            root_name: self
                .part
                .clusters
                .iter()
                .find(|def| def.id == self.part.root)
                .map_or("", |def| def.name.as_str()),
            definition_count: self.part.clusters.len(),
            node_count: self
                .part
                .clusters
                .iter()
                .map(|def| def.graph.nodes.len())
                .sum(),
            part_bytes: self.envelope.part_json.len(),
            evaluation_performed: false,
        }
    }

    pub fn part_bytes(&self) -> &[u8] {
        self.envelope.part_json.as_bytes()
    }

    /// The existing transactional importer renumbers definitions. No evaluation
    /// occurs here, and package identity is not written into the legacy graph.
    pub fn import_into(&self, graph: &mut Graph) -> Result<u64, String> {
        cluster::import_part(graph, self.part_bytes())
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        Self::decode(&read_regular(path, MAX_PACKAGE_BYTES)?)
    }

    pub fn pack_files(manifest: &Path, part: &Path) -> Result<Self, String> {
        let manifest: Manifest =
            serde_json::from_slice(&read_regular(manifest, MAX_MANIFEST_BYTES)?)
                .map_err(|e| format!("Manifest JSON が正しくありません（{} 行目）。", e.line()))?;
        Self::from_part(manifest, &read_regular(part, cluster::MAX_PART_BYTES)?)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let bytes = self.encode()?;
        crate::persistence::atomic_write(path, |file| file.write_all(&bytes))
    }

    pub fn unpack_part(&self, path: &Path) -> Result<(), String> {
        crate::persistence::atomic_write(path, |file| file.write_all(self.part_bytes()))
    }
}

fn read_regular(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let invalid = || "通常のファイルを指定してください。".to_string();
    let too_large = || format!("ファイルの上限は {limit} バイトです。");
    let io = |e: std::io::Error| format!("ファイルを読めません: {e}");
    let metadata = fs::metadata(path).map_err(io)?;
    if !metadata.is_file() {
        return Err(invalid());
    }
    if metadata.len() > limit as u64 {
        return Err(too_large());
    }
    let file = File::open(path).map_err(io)?;
    if !file.metadata().map_err(io)?.is_file() {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > limit {
        return Err(too_large());
    }
    Ok(bytes)
}
