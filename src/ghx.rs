//! Read-only Grasshopper XML inspection and deliberately narrow, explicit conversion.
//! No Rhino SDK, script execution, network access or plugin loading.
use crate::model::{Graph, MAX_NODES, NodeId, NodeKind};
use crate::persistence::{self, Project};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use xml::reader::{EventReader, XmlEvent};

pub const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_DEPTH: usize = 64;
const MAX_ELEMENTS: usize = 100_000;
const MAX_WIRES: usize = 32_768;
const SLIDER: &str = "57da07bd-ecab-415d-9d86-af36d7073abc";
const POINT: &str = "3581f42a-9592-4549-bd6b-1c0fc39d067b";
type Res<T> = Result<T, String>;

#[derive(Clone, Debug, Serialize)]
struct Element {
    tag: String,
    attributes: BTreeMap<String, String>,
    text: String,
    children: Vec<Element>,
}

impl Element {
    fn named<'a>(
        &'a self,
        wrapper: &'static str,
        tag: &'static str,
    ) -> impl Iterator<Item = &'a Element> + 'a {
        self.children
            .iter()
            .filter(move |e| e.tag == wrapper)
            .flat_map(|e| &e.children)
            .filter(move |e| e.tag == tag)
    }

    fn chunks(&self) -> impl Iterator<Item = &Element> {
        self.named("chunks", "chunk")
    }

    fn items(&self) -> impl Iterator<Item = &Element> {
        self.named("items", "item")
    }

    fn name(&self) -> &str {
        self.attributes
            .get("name")
            .map(String::as_str)
            .unwrap_or("")
    }

    fn index(&self) -> Res<usize> {
        self.attributes
            .get("index")
            .ok_or_else(|| format!("missing index: {}", self.name()))?
            .parse()
            .map_err(|_| format!("invalid index: {}", self.name()))
    }

    fn chunk(&self, name: &str) -> Res<Option<&Element>> {
        unique(self.chunks().filter(|e| e.name() == name), name)
    }

    fn item(&self, name: &str) -> Res<Option<&Element>> {
        unique(self.items().filter(|e| e.name() == name), name)
    }

    fn field(&self, name: &str) -> Res<Option<&str>> {
        match self.item(name)? {
            Some(e) if e.children.is_empty() => Ok(Some(e.text.trim())),
            Some(_) => Err(format!("expected scalar item {name}")),
            None => Ok(None),
        }
    }

    fn required(&self, name: &str) -> Res<&str> {
        self.field(name)?
            .ok_or_else(|| format!("missing item {name}"))
    }
}

fn unique<'a>(
    mut entries: impl Iterator<Item = &'a Element>,
    name: &str,
) -> Res<Option<&'a Element>> {
    let first = entries.next();
    if entries.next().is_some() {
        return Err(format!("duplicate field {name}"));
    }
    Ok(first)
}

fn xml_tree(bytes: &[u8]) -> Res<Element> {
    if bytes.len() > MAX_BYTES {
        return Err("GHX exceeds 8 MiB".into());
    }
    let source = std::str::from_utf8(bytes).map_err(|_| "GHX must be UTF-8")?;
    // Reject declarations before the XML reader sees them, including internal DTDs.
    // A literal occurrence in a comment/CDATA is also rejected (fail closed).
    if source.contains('\0') || source.contains("<!DOCTYPE") || source.contains("<!ENTITY") {
        return Err("NUL, DTD and entity declarations are forbidden".into());
    }
    let mut stack: Vec<Element> = Vec::new();
    let mut root = None;
    let mut count = 0usize;
    for event in EventReader::new(bytes) {
        match event.map_err(|e| format!("invalid GHX XML: {e}"))? {
            XmlEvent::StartDocument { encoding, .. } => {
                if !encoding.eq_ignore_ascii_case("utf-8") {
                    return Err("only UTF-8 GHX is supported".into());
                }
            }
            XmlEvent::StartElement {
                name, attributes, ..
            } => {
                count += 1;
                if count > MAX_ELEMENTS || stack.len() >= MAX_DEPTH || attributes.len() > 32 {
                    return Err("GHX XML complexity limit exceeded".into());
                }
                if name.prefix.is_some() || name.namespace.is_some() {
                    return Err("namespaced GHX elements are not supported".into());
                }
                let mut attrs = BTreeMap::new();
                for a in attributes {
                    if a.name.prefix.is_some()
                        || a.name.namespace.is_some()
                        || attrs.insert(a.name.local_name, a.value).is_some()
                    {
                        return Err("invalid or duplicate XML attribute".into());
                    }
                }
                stack.push(Element {
                    tag: name.local_name,
                    attributes: attrs,
                    text: String::new(),
                    children: Vec::new(),
                });
            }
            XmlEvent::EndElement { .. } => {
                let elem = stack.pop().ok_or("unbalanced XML")?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(elem);
                } else if root.replace(elem).is_some() {
                    return Err("multiple XML roots".into());
                }
            }
            XmlEvent::Characters(s) | XmlEvent::CData(s) | XmlEvent::Whitespace(s) => {
                if let Some(e) = stack.last_mut() {
                    e.text.push_str(&s);
                } else if !s.trim().is_empty() {
                    return Err("text outside archive".into());
                }
            }
            XmlEvent::ProcessingInstruction { .. } => {
                return Err("XML processing instructions are forbidden".into());
            }
            XmlEvent::EndDocument => break,
            XmlEvent::Comment(_) => {}
            // xml-rs versions with additional declaration events: reject them.
            #[allow(unreachable_patterns)]
            _ => return Err("unsupported XML event".into()),
        }
    }
    if !stack.is_empty() {
        return Err("truncated GHX XML".into());
    }
    let root = root.ok_or("empty GHX XML")?;
    if root.tag != "Archive" {
        return Err("expected a GHX Archive".into());
    }
    Ok(root)
}

fn guid(s: &str) -> Res<String> {
    let s = s.trim();
    if s.len() != 36
        || !s.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        return Err("invalid GHX GUID".into());
    }
    Ok(s.to_ascii_lowercase())
}

fn finite(s: &str) -> Res<f64> {
    let v: f64 = s.trim().parse().map_err(|_| "invalid GHX number")?;
    if !v.is_finite() {
        return Err("non-finite GHX number".into());
    }
    Ok(v)
}

fn bool_field(e: &Element, name: &str) -> Res<Option<bool>> {
    e.field(name)?
        .map(|s| match s.to_ascii_lowercase().as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(format!("invalid Boolean {name}")),
        })
        .transpose()
}

fn position(e: &Element) -> Res<Option<[f32; 2]>> {
    let Some(attr) = e.chunk("Attributes")? else {
        return Ok(None);
    };
    let Some(pivot) = attr.item("Pivot")?.or(attr.item("Bounds")?) else {
        return Ok(None);
    };
    let coord = |tag| -> Res<f32> {
        let v = finite(
            &unique(pivot.children.iter().filter(|v| v.tag == tag), tag)?
                .ok_or("missing pivot coordinate")?
                .text,
        )?;
        if v.abs() > 1_000_000.0 {
            return Err("pivot out of range".into());
        }
        Ok(v as f32)
    };
    Ok(Some([coord("X")?, coord("Y")?]))
}

fn indexed<'a>(entries: impl Iterator<Item = &'a Element>) -> Res<Vec<&'a Element>> {
    let mut ordered = BTreeMap::new();
    for entry in entries {
        if ordered.insert(entry.index()?, entry).is_some() {
            return Err(format!("duplicate index: {}", entry.name()));
        }
    }
    if !ordered.keys().copied().eq(0..ordered.len()) {
        return Err("non-contiguous GHX indices".into());
    }
    Ok(ordered.into_values().collect())
}

fn refs(e: &Element, name: &str, count_name: &str) -> Res<Vec<String>> {
    let entries = indexed(e.items().filter(|item| item.name() == name))?;
    if let Some(count) = e.field(count_name)?
        && count.parse::<usize>().ok() != Some(entries.len())
    {
        return Err(format!("inconsistent {count_name}"));
    }
    entries
        .into_iter()
        .map(|item| {
            if !item.children.is_empty() {
                return Err("invalid GUID item".into());
            }
            guid(&item.text)
        })
        .collect()
}

#[derive(Debug, Serialize)]
pub struct Port {
    pub index: Option<usize>,
    pub instance_guid: String,
    pub name: String,
    pub nickname: String,
    pub sources: Vec<String>,
    // Serialized GHX settings and persistent data; no deserialization into executable values.
    details: Element,
}

fn port(e: &Element, index: Option<usize>) -> Res<Port> {
    Ok(Port {
        index,
        instance_guid: guid(e.required("InstanceGuid")?)?,
        name: e.field("Name")?.unwrap_or("").into(),
        nickname: e.field("NickName")?.unwrap_or("").into(),
        sources: refs(e, "Source", "SourceCount")?,
        details: e.clone(),
    })
}

#[derive(Debug, Serialize)]
pub struct Object {
    pub index: usize,
    pub component_guid: String,
    pub instance_guid: String,
    pub name: String,
    pub nickname: String,
    pub position: Option<[f32; 2]>,
    pub preview: Option<bool>,
    pub slider_value: Option<f64>,
    pub toggle_value: Option<bool>,
    pub panel_text: Option<String>,
    pub group_members: Vec<String>,
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    pub candidate_node: Option<NodeKind>,
    details: Element,
}

#[derive(Debug, Serialize)]
pub struct Connection {
    pub source_guid: String,
    pub source_object: Option<usize>,
    pub source_port: Option<usize>,
    pub target_object: usize,
    pub target_port: Option<usize>,
    pub source_order: usize,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub format: &'static str,
    pub objects: Vec<Object>,
    pub connections: Vec<Connection>,
    pub diagnostics: Vec<String>,
    // Preserve the exact source, including unfamiliar component or script payloads.
    // This is inert text; it is never used by the evaluator.
    pub source_xml: String,
}

fn candidate(guid: &str) -> Option<NodeKind> {
    match guid {
        SLIDER => Some(NodeKind::Number),
        POINT => Some(NodeKind::Point),
        _ => None,
    }
}

pub fn inspect(bytes: &[u8]) -> Res<Report> {
    let root = xml_tree(bytes)?;
    let definition = root.chunk("Definition")?.ok_or("missing Definition")?;
    let direct = definition.chunk("DefinitionObjects")?;
    let nested = match definition.chunk("DefinitionProperties")? {
        Some(p) => p.chunk("DefinitionObjects")?,
        None => None,
    };
    let collection = match (direct, nested) {
        (Some(e), None) | (None, Some(e)) => e,
        _ => return Err("missing or ambiguous DefinitionObjects".into()),
    };
    if collection.chunks().any(|e| e.name() != "Object") {
        return Err("unrecognized DefinitionObjects structure".into());
    }
    let entries = indexed(collection.chunks())?;
    if entries.len() > MAX_NODES {
        return Err("too many GHX objects".into());
    }
    if let Some(count) = collection.field("ObjectCount")?
        && count.parse::<usize>().ok() != Some(entries.len())
    {
        return Err("inconsistent ObjectCount".into());
    }
    allowed(collection, &["ObjectCount"], &["Object"])?;
    let mut objects = Vec::new();
    let mut diagnostics = Vec::new();
    for (index, obj) in entries.into_iter().enumerate() {
        let c = obj.chunk("Container")?.ok_or("missing object Container")?;
        let component_guid = guid(obj.required("GUID")?)?;
        let instance_guid = guid(c.required("InstanceGuid")?)?;
        let name = obj
            .field("Name")?
            .or(c.field("Name")?)
            .unwrap_or("")
            .to_string();
        let nickname = c.field("NickName")?.unwrap_or("").to_string();
        let mut inputs: Vec<Port> = indexed(c.chunks().filter(|e| e.name() == "param_input"))?
            .into_iter()
            .enumerate()
            .map(|(i, e)| port(e, Some(i)))
            .collect::<Res<_>>()?;
        let mut outputs: Vec<Port> = indexed(c.chunks().filter(|e| e.name() == "param_output"))?
            .into_iter()
            .enumerate()
            .map(|(i, e)| port(e, Some(i)))
            .collect::<Res<_>>()?;
        let group_members = refs(c, "ID", "ID_Count")?;
        // Stand-alone parameter objects identify their sources/outputs by their own GUID.
        let direct_sources = refs(c, "Source", "SourceCount")?;
        if !direct_sources.is_empty() {
            inputs.push(port(c, None)?);
        }
        if outputs.is_empty() && name != "Group" {
            let mut standalone = port(c, None)?;
            standalone.sources.clear();
            outputs.push(standalone);
        }
        let slider_value = match c.chunk("Slider")? {
            Some(slider) => slider.field("Value")?.map(finite).transpose()?,
            None => None,
        };
        let candidate_node = candidate(&component_guid);
        if candidate_node.is_none() {
            diagnostics.push(format!(
                "unsupported object #{index} {name} ({component_guid})"
            ));
        }
        objects.push(Object {
            index,
            component_guid,
            instance_guid,
            name,
            nickname,
            position: position(c)?,
            preview: bool_field(c, "Hidden")?.map(|b| !b),
            slider_value,
            toggle_value: bool_field(c, "ToggleValue")?,
            panel_text: c.item("UserText")?.map(|e| e.text.clone()),
            group_members,
            inputs,
            outputs,
            candidate_node,
            details: obj.clone(),
        });
    }
    let mut all_ids = BTreeSet::new();
    let mut output_owner = BTreeMap::new();
    for object in &objects {
        if !all_ids.insert(object.instance_guid.clone()) {
            return Err("duplicate object GUID".into());
        }
        for p in object.inputs.iter().chain(&object.outputs) {
            if p.index.is_some() && !all_ids.insert(p.instance_guid.clone()) {
                return Err("duplicate port GUID".into());
            }
        }
        for p in &object.outputs {
            if output_owner
                .insert(p.instance_guid.clone(), (object.index, p.index))
                .is_some()
            {
                return Err("duplicate output GUID".into());
            }
        }
    }
    let mut connections = Vec::new();
    let mut parents: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); objects.len()];
    for obj in &objects {
        for p in &obj.inputs {
            for (source_order, source_guid) in p.sources.iter().enumerate() {
                if connections.len() >= MAX_WIRES {
                    return Err("too many GHX wires".into());
                }
                let owner = output_owner.get(source_guid).copied();
                if let Some((i, _)) = owner {
                    parents[obj.index].insert(i);
                } else {
                    diagnostics.push(format!(
                        "unresolved source {source_guid} for object #{}",
                        obj.index
                    ));
                }
                connections.push(Connection {
                    source_guid: source_guid.clone(),
                    source_object: owner.map(|p| p.0),
                    source_port: owner.and_then(|p| p.1),
                    target_object: obj.index,
                    target_port: p.index,
                    source_order,
                });
            }
        }
    }
    // Iterative cycle detection, including unsupported nodes where wires are known.
    let mut indegree: Vec<_> = parents.iter().map(BTreeSet::len).collect();
    let mut after = vec![Vec::new(); objects.len()];
    for (child, sources) in parents.iter().enumerate() {
        for &source in sources {
            after[source].push(child);
        }
    }
    let mut queue: VecDeque<_> = indegree
        .iter()
        .enumerate()
        .filter(|(_, d)| **d == 0)
        .map(|(i, _)| i)
        .collect();
    let mut seen = 0;
    while let Some(i) = queue.pop_front() {
        seen += 1;
        for &child in &after[i] {
            indegree[child] -= 1;
            if indegree[child] == 0 {
                queue.push_back(child);
            }
        }
    }
    if seen != objects.len() {
        diagnostics.push("cyclic GHX graph; conversion forbidden".into());
    }
    Ok(Report {
        format: "nodes-ghx-inspection-v1",
        objects,
        connections,
        diagnostics,
        source_xml: std::str::from_utf8(bytes)
            .map_err(|_| "invalid UTF-8")?
            .to_owned(),
    })
}

fn allowed(e: &Element, items: &[&str], chunks: &[&str]) -> Res<()> {
    if let Some(i) = e.items().find(|i| !items.contains(&i.name())) {
        return Err(format!("unmapped setting {}", i.name()));
    }
    if let Some(c) = e.chunks().find(|c| !chunks.contains(&c.name())) {
        return Err(format!("unmapped chunk {}", c.name()));
    }
    if e.children
        .iter()
        .any(|c| c.tag != "items" && c.tag != "chunks")
        || e.children.iter().filter(|c| c.tag == "items").count() > 1
        || e.children.iter().filter(|c| c.tag == "chunks").count() > 1
        || e.children.iter().any(|c| {
            c.children.iter().any(|v| {
                (c.tag == "items" && v.tag != "item") || (c.tag == "chunks" && v.tag != "chunk")
            })
        })
    {
        return Err("unmapped XML structure".into());
    }
    Ok(())
}

fn scalar_default(e: &Element) -> Res<Option<f64>> {
    let Some(p) = e.chunk("PersistentData")? else {
        return Ok(None);
    };
    allowed(p, &["Count"], &["Branch"])?;
    let branches = indexed(p.chunks())?;
    if branches.is_empty() && p.required("Count")? == "0" {
        return Ok(None);
    }
    if branches.len() != 1 || p.required("Count")? != "1" {
        return Err("non-scalar persistent data".into());
    }
    let b = branches[0];
    allowed(b, &["Count", "Path"], &["Item"])?;
    if b.required("Path")? != "{0}" || b.required("Count")? != "1" {
        return Err("non-scalar persistent path".into());
    }
    let items = indexed(b.chunks())?;
    if items.len() != 1 {
        return Err("non-scalar persistent data".into());
    }
    allowed(items[0], &["number"], &[])?;
    Ok(Some(finite(items[0].required("number")?)?))
}

fn set_number(graph: &mut Graph, id: NodeId, key: &str, value: f64) -> Res<()> {
    let n = graph.node_mut(id).ok_or("missing imported node")?;
    let spec = n
        .kind
        .spec()
        .inputs
        .iter()
        .find(|s| s.key == key && s.default.is_some())
        .ok_or("missing numeric input")?;
    if !value.is_finite()
        || value < spec.min
        || value > spec.max
        || (spec.integer && value.fract() != 0.0)
    {
        return Err(format!("numeric value out of NODES range: {key}"));
    }
    n.params.insert(key.into(), value);
    Ok(())
}

/// Strict, all-or-nothing scalar conversion. Unsupported objects are never dropped or executed.
/// Inspect first to retain information that cannot be represented by the one-output native graph.
pub fn convert(bytes: &[u8]) -> Res<Project> {
    let report = inspect(bytes)?;
    if !report.diagnostics.is_empty() {
        return Err(format!(
            "conversion refused; run --inspect-ghx: {}",
            report.diagnostics[0]
        ));
    }
    if report.objects.is_empty() {
        return Err("empty GHX graph".into());
    }
    let mut graph = Graph::default();
    let mut ids = Vec::new();
    for object in &report.objects {
        let kind = object.candidate_node.ok_or("unsupported component")?;
        let c = object
            .details
            .chunk("Container")?
            .ok_or("missing Container")?;
        allowed(&object.details, &["GUID", "Name", "Lib"], &["Container"])?;
        if let Some(lib) = object.details.field("Lib")?
            && guid(lib)? != "00000000-0000-0000-0000-000000000000"
        {
            return Err("external component library is not convertible".into());
        }
        if let Some(attributes) = c.chunk("Attributes")? {
            allowed(attributes, &["Bounds", "Pivot"], &[])?;
        }
        allowed(
            c,
            &[
                "Description",
                "InstanceGuid",
                "Name",
                "NickName",
                "Optional",
                "SourceCount",
                "Hidden",
                "Locked",
            ],
            &["Attributes", "Slider", "param_input", "param_output"],
        )?;
        if bool_field(c, "Locked")?.unwrap_or(false) {
            return Err("locked node requires manual migration".into());
        }
        let id = graph.add(kind, object.position.ok_or("missing object pivot")?)?;
        let n = graph.node_mut(id).ok_or("missing imported node")?;
        n.name = if object.nickname.is_empty() {
            object.name.clone()
        } else {
            object.nickname.clone()
        };
        n.preview = object.preview.unwrap_or(true);
        if kind == NodeKind::Number {
            if !object.inputs.is_empty()
                || object.outputs.len() != 1
                || object.outputs[0].index.is_some()
                || c.chunks()
                    .any(|e| e.name() == "param_input" || e.name() == "param_output")
            {
                return Err("unexpected slider port structure".into());
            }
            let slider = c.chunk("Slider")?.ok_or("missing Slider")?;
            allowed(
                slider,
                &[
                    "Digits",
                    "GripDisplay",
                    "Interval",
                    "Max",
                    "Min",
                    "SnapCount",
                    "Value",
                ],
                &[],
            )?;
            let value = object.slider_value.ok_or("missing slider value")?;
            let min = finite(slider.required("Min")?)?;
            let max = finite(slider.required("Max")?)?;
            if min > max || value < min || value > max {
                return Err("invalid slider range".into());
            }
            set_number(&mut graph, id, "value", value)?;
        } else {
            if c.chunk("Slider")?.is_some()
                || object.inputs.len() != 3
                || object.outputs.len() != 1
                || object.outputs[0].index != Some(0)
                || !object.outputs[0].sources.is_empty()
            {
                return Err("unexpected Construct Point port structure".into());
            }
            allowed(
                &object.outputs[0].details,
                &[
                    "Description",
                    "InstanceGuid",
                    "Name",
                    "NickName",
                    "Optional",
                    "SourceCount",
                ],
                &["Attributes"],
            )?;
            for (i, p) in object.inputs.iter().enumerate() {
                if p.index != Some(i)
                    || p.sources.len() > 1
                    || (p.name != ["X", "Y", "Z"][i]
                        && p.name != ["X coordinate", "Y coordinate", "Z coordinate"][i])
                {
                    return Err("unsupported point input layout".into());
                }
                allowed(
                    &p.details,
                    &[
                        "Description",
                        "InstanceGuid",
                        "Name",
                        "NickName",
                        "Optional",
                        "Source",
                        "SourceCount",
                    ],
                    &["Attributes", "PersistentData"],
                )?;
                if let Some(value) = scalar_default(&p.details)? {
                    set_number(&mut graph, id, ["x", "y", "z"][i], value)?;
                } else if p.sources.is_empty() {
                    return Err("missing explicit scalar point default".into());
                }
            }
        }
        ids.push(id);
    }
    for wire in &report.connections {
        let source = wire.source_object.ok_or("unresolved source")?;
        let target_port = wire
            .target_port
            .ok_or("stand-alone parameter input not convertible")?;
        if report.objects[wire.target_object].candidate_node != Some(NodeKind::Point)
            || report.objects[source].candidate_node != Some(NodeKind::Number)
            || wire.source_port.is_some()
            || wire.source_order != 0
            || target_port >= 3
        {
            return Err("unsupported wire mapping".into());
        }
        graph.connect(
            ids[source],
            ids[wire.target_object],
            ["x", "y", "z"][target_port],
        )?;
    }
    let project = Project::new("Imported GHX (scalar subset)", graph);
    project.validate()?;
    Ok(project)
}

fn extension(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(|v| v.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case(expected))
}

fn read_ghx(path: &Path) -> Res<Vec<u8>> {
    if !extension(path, "ghx") {
        return Err("only .ghx is supported; save .gh as GHX first".into());
    }
    if !std::fs::metadata(path)
        .map_err(|e| e.to_string())?
        .is_file()
    {
        return Err("GHX source must be a regular file".into());
    }
    let file = File::open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("GHX source must be a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("GHX exceeds 8 MiB".into());
    }
    Ok(bytes)
}

pub fn inspect_file(input: &Path, output: &Path) -> Res<()> {
    if !extension(output, "json") {
        return Err("inspection output must be .json".into());
    }
    let report = inspect(&read_ghx(input)?)?;
    // Bound serialized output independently of the input; keep the old destination on failure.
    persistence::atomic_write(output, |file| {
        let mut writer = BoundedWriter {
            inner: file,
            remaining: 64 * 1024 * 1024,
        };
        serde_json::to_writer(&mut writer, &report).map_err(std::io::Error::other)?;
        writer.write_all(b"\n")
    })
}

struct BoundedWriter<W> {
    inner: W,
    remaining: usize,
}
impl<W: Write> Write for BoundedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.remaining {
            return Err(std::io::Error::other("GHX report exceeds 64 MiB"));
        }
        let n = self.inner.write(bytes)?;
        self.remaining -= n;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

pub fn convert_file(input: &Path, output: &Path) -> Res<()> {
    if !extension(output, "morph") {
        return Err("conversion output must be .morph".into());
    }
    let project = convert(&read_ghx(input)?)?;
    persistence::save(output, &project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{engine::Evaluator, model::Value};
    const SAMPLE: &[u8] = include_bytes!("../examples/interop/scalar-point.ghx");

    #[test]
    fn source_and_port_identity_and_scalar_evaluation() {
        let report = inspect(SAMPLE).unwrap();
        assert_eq!(report.objects.len(), 2);
        assert_eq!(report.connections.len(), 1);
        assert_eq!(report.connections[0].source_object, Some(0));
        assert_eq!(report.connections[0].target_port, Some(0));
        assert_eq!(report.source_xml.as_bytes(), SAMPLE);
        let project = convert(SAMPLE).unwrap();
        let evaluation = Evaluator::default().evaluate(&project.graph, &|| false);
        let point = evaluation.outputs[&project.graph.nodes[1].id]
            .as_ref()
            .unwrap();
        assert_eq!(
            point.branches.values().flatten().next(),
            Some(&Value::Point([7., 2., 3.]))
        );
    }

    #[test]
    fn unsupported_object_preserves_wire_and_source() {
        let xml = std::str::from_utf8(SAMPLE)
            .unwrap()
            .replace(POINT, "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa");
        let report = inspect(xml.as_bytes()).unwrap();
        assert_eq!(report.objects.len(), 2);
        assert_eq!(report.connections.len(), 1);
        assert_eq!(report.source_xml, xml);
        assert!(!report.diagnostics.is_empty());
        assert!(convert(xml.as_bytes()).is_err());
    }

    #[test]
    fn invalid_xml_and_limits() {
        assert!(inspect(b"<!DOCTYPE Archive [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><Archive>&x;</Archive>").is_err());
        assert!(inspect(b"<Archive><chunks>").is_err());
        assert!(inspect(&vec![b' '; MAX_BYTES + 1]).is_err());
        let deep = format!(
            "{}{}",
            "<x>".repeat(MAX_DEPTH + 1),
            "</x>".repeat(MAX_DEPTH + 1)
        );
        assert!(inspect(deep.as_bytes()).is_err());
        assert!(inspect(b"<Archive/><!--x--><Archive/>").is_err());
    }

    #[test]
    fn unexpected_tree_settings_and_multiple_wires_are_not_silently_converted() {
        let text = std::str::from_utf8(SAMPLE).unwrap();
        let graft = text.replace(
            "<item name=\"Name\">X</item>",
            "<item name=\"Name\">X</item><item name=\"DataMapping\">2</item>",
        );
        assert!(inspect(graft.as_bytes()).is_ok());
        assert!(convert(graft.as_bytes()).is_err());
        let multi = text.replace("<item name=\"SourceCount\">1</item>",
            "<item name=\"SourceCount\">2</item><item name=\"Source\" index=\"1\">11111111-1111-1111-1111-111111111111</item>");
        assert_eq!(inspect(multi.as_bytes()).unwrap().connections.len(), 2);
        assert!(convert(multi.as_bytes()).is_err());
    }

    #[test]
    fn invalid_numbers_dangling_wires_and_cycles() {
        let text = std::str::from_utf8(SAMPLE).unwrap();
        assert!(convert(text.replace(">7</item>", ">NaN</item>").as_bytes()).is_err());
        let missing = text.replace(
            "name=\"Source\" index=\"0\">11111111-1111-1111-1111-111111111111",
            "name=\"Source\" index=\"0\">99999999-9999-9999-9999-999999999999",
        );
        let report = inspect(missing.as_bytes()).unwrap();
        assert_eq!(report.connections[0].source_object, None);
        assert!(convert(missing.as_bytes()).is_err());
        let cycle = text.replace(
            "name=\"Source\" index=\"0\">11111111-1111-1111-1111-111111111111",
            "name=\"Source\" index=\"0\">55555555-5555-5555-5555-555555555555",
        );
        assert!(
            inspect(cycle.as_bytes())
                .unwrap()
                .diagnostics
                .iter()
                .any(|s| s.contains("cyclic"))
        );
        assert!(convert(cycle.as_bytes()).is_err());
    }
}
