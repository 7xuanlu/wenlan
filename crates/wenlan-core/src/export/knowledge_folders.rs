// SPDX-License-Identifier: Apache-2.0
//! Physical organization of the Page projection. This module never changes DB scope.
use super::*;
use unicode_normalization::UnicodeNormalization as _;

const MOVE_JOURNAL: &str = ".page-move-v1.json";
const TREE_LIMIT: usize = 50_000;
const DEPTH_LIMIT: usize = 32;

pub(super) fn deserialize_pages<'de, D>(
    deserializer: D,
) -> Result<HashMap<String, PageFileState>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Pages;
    impl<'de> serde::de::Visitor<'de> for Pages {
        type Value = HashMap<String, PageFileState>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("unique Page mappings")
        }
        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>,
        {
            let mut pages = HashMap::new();
            while let Some((id, entry)) = map.next_entry::<String, PageFileState>()? {
                if pages.len() >= TREE_LIMIT || pages.insert(id, entry).is_some() {
                    return Err(serde::de::Error::custom(
                        "duplicate or excessive Page mappings",
                    ));
                }
            }
            Ok(pages)
        }
    }
    deserializer.deserialize_map(Pages)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnowledgeFolderEntry {
    pub path: String,
    pub parent_path: String,
    pub name: String,
}

fn invalid_path() -> WenlanError {
    WenlanError::Validation("knowledge_folder_path_invalid".into())
}

fn reserved_directory(name: &str) -> bool {
    name.starts_with('.')
        || name.eq_ignore_ascii_case("archive")
        || name.eq_ignore_ascii_case("_sources")
}

fn validate_component(name: &str) -> Result<(), WenlanError> {
    if name.is_empty()
        || name.len() > 255
        || name.starts_with('.')
        || name.ends_with(['.', ' '])
        || name.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*')
        })
    {
        return Err(invalid_path());
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let port = stem
        .strip_prefix("COM")
        .or_else(|| stem.strip_prefix("LPT"));
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || port.is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    {
        return Err(invalid_path());
    }
    Ok(())
}

/// Validate a portable, root-relative folder path. The empty string names the root.
pub fn validate_knowledge_folder_path(path: &str) -> Result<String, WenlanError> {
    if path.is_empty() {
        return Ok(String::new());
    }
    if path.len() > 4096 || path.split('/').count() > DEPTH_LIMIT {
        return Err(invalid_path());
    }
    for component in path.split('/') {
        validate_component(component)?;
        if reserved_directory(component) {
            return Err(invalid_path());
        }
    }
    Ok(path.to_string())
}

pub(super) fn validate_new_page_path(path: &str) -> Result<(), WenlanError> {
    if path.len() > 4096 {
        return Err(invalid_path());
    }
    let (folder, name) = path.rsplit_once('/').unwrap_or(("", path));
    validate_knowledge_folder_path(folder)?;
    validate_component(name)?;
    if !name.ends_with(".md") {
        return Err(invalid_path());
    }
    Ok(())
}

// Existing local names are not new portable-path requests. Keep old allocations
// (including `con.md` and `.md`) and user directories without relaxing traversal.
fn validate_existing_component(name: &str) -> Result<(), WenlanError> {
    if name.is_empty()
        || matches!(name, "." | "..")
        || name.len() > 255
        || name
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\'))
    {
        return Err(invalid_path());
    }
    Ok(())
}

fn validate_existing_folder_path(path: &str) -> Result<(), WenlanError> {
    if path.is_empty() {
        return Ok(());
    }
    if path.len() > 4096 || path.split('/').count() > DEPTH_LIMIT {
        return Err(invalid_path());
    }
    for component in path.split('/') {
        validate_existing_component(component)?;
        if reserved_directory(component) {
            return Err(invalid_path());
        }
    }
    Ok(())
}

pub(super) fn validate_page_path(path: &str) -> Result<(), WenlanError> {
    if path.len() > 4096 {
        return Err(invalid_path());
    }
    let (folder, name) = path.rsplit_once('/').unwrap_or(("", path));
    validate_existing_folder_path(folder)?;
    validate_existing_component(name)?;
    if !name.ends_with(".md") {
        return Err(invalid_path());
    }
    Ok(())
}

pub(super) fn open_existing_folder(root: &Dir, path: &str) -> Result<Dir, WenlanError> {
    validate_existing_folder_path(path)?;
    let mut directory = root.try_clone()?;
    for component in path.split('/').filter(|part| !part.is_empty()) {
        ensure_case_available(&directory, component, true)?;
        directory = directory.open_dir_nofollow(component)?;
    }
    Ok(directory)
}

pub(super) fn open_folder(root: &Dir, path: &str) -> Result<Dir, WenlanError> {
    validate_knowledge_folder_path(path)?;
    let mut directory = root.try_clone()?;
    for component in path.split('/').filter(|part| !part.is_empty()) {
        // Refuse ambiguous case spellings even on a case-sensitive host.
        ensure_case_available(&directory, component, true)?;
        directory = directory.open_dir_nofollow(component)?;
    }
    Ok(directory)
}

pub(super) fn page_parent(root: &Dir, path: &str) -> Result<(Dir, String), WenlanError> {
    validate_page_path(path)?;
    let (folder, name) = path.rsplit_once('/').unwrap_or(("", path));
    Ok((open_existing_folder(root, folder)?, name.to_string()))
}

pub(super) fn ensure_case_available(
    directory: &Dir,
    name: &str,
    allow_exact: bool,
) -> Result<(), WenlanError> {
    let wanted = collision_key(name);
    for (index, entry) in directory.entries()?.enumerate() {
        if index >= TREE_LIMIT {
            return Err(WenlanError::Conflict("knowledge_tree_limit".into()));
        }
        let entry = entry?;
        let existing = entry.file_name();
        if collision_key(&existing.to_string_lossy()) == wanted
            && (!allow_exact || existing != OsStr::new(name))
        {
            return Err(WenlanError::Conflict("knowledge_path_collision".into()));
        }
    }
    Ok(())
}

pub(super) fn collision_key(name: &str) -> String {
    name.nfc().flat_map(char::to_uppercase).collect()
}

pub(super) fn scan_tree(
    root: &Dir,
) -> Result<(Vec<KnowledgeFolderEntry>, Vec<String>), WenlanError> {
    fn walk(
        directory: &Dir,
        parent: &str,
        depth: usize,
        count: &mut usize,
        folders: &mut Vec<KnowledgeFolderEntry>,
        pages: &mut Vec<String>,
    ) -> Result<(), WenlanError> {
        if depth > DEPTH_LIMIT {
            return Err(WenlanError::Conflict("knowledge_tree_limit".into()));
        }
        let mut entries = directory
            .entries()?
            .take(TREE_LIMIT + 1)
            .collect::<Result<Vec<_>, _>>()?;
        if entries.len() > TREE_LIMIT {
            return Err(WenlanError::Conflict("knowledge_tree_limit".into()));
        }
        entries.sort_by_key(|entry| entry.file_name());
        let mut names = std::collections::HashSet::new();
        for entry in entries {
            *count += 1;
            if *count > TREE_LIMIT {
                return Err(WenlanError::Conflict("knowledge_tree_limit".into()));
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                return Err(invalid_path());
            };
            let metadata = directory.symlink_metadata(name)?;
            if metadata.is_dir() && reserved_directory(name) {
                continue;
            }
            if !metadata.is_dir() && !name.ends_with(".md") {
                continue;
            }
            validate_existing_component(name)?;
            if !names.insert(collision_key(name)) {
                return Err(WenlanError::Conflict("knowledge_path_collision".into()));
            }
            // Never follow external directory/file links.
            if metadata.file_type().is_symlink() {
                continue;
            }
            let path = if parent.is_empty() {
                name.to_string()
            } else {
                format!("{parent}/{name}")
            };
            if metadata.is_dir() {
                folders.push(KnowledgeFolderEntry {
                    path: path.clone(),
                    parent_path: parent.to_string(),
                    name: name.to_string(),
                });
                walk(
                    &directory.open_dir_nofollow(name)?,
                    &path,
                    depth + 1,
                    count,
                    folders,
                    pages,
                )?;
            } else if metadata.is_file() && name.ends_with(".md") {
                pages.push(path);
            }
        }
        Ok(())
    }
    let (mut folders, mut pages) = (Vec::new(), Vec::new());
    walk(root, "", 0, &mut 0, &mut folders, &mut pages)?;
    Ok((folders, pages))
}

pub(super) fn read_state_strict(control: &Dir) -> Result<KnowledgeState, WenlanError> {
    let mut budget = RepairReadBudget::new();
    let Some((bytes, _, _)) = read_optional_projection_state_identity_nofollow(
        control,
        OsStr::new("state.json"),
        &mut budget,
    )?
    else {
        return Ok(KnowledgeState::default());
    };
    let json: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| WenlanError::Conflict("page_projection_state_invalid".into()))?;
    let state = if json.get("concepts").is_some() && json.get("pages").is_none() {
        let legacy: LegacyKnowledgeStateV1 = serde_json::from_slice(&bytes)
            .map_err(|_| WenlanError::Conflict("page_projection_state_invalid".into()))?;
        KnowledgeState {
            schema_version: KNOWLEDGE_STATE_SCHEMA_V2,
            pages: legacy
                .concepts
                .into_iter()
                .map(|(id, entry)| {
                    (
                        id.strip_prefix("concept_")
                            .map(|suffix| format!("page_{suffix}"))
                            .unwrap_or(id),
                        entry,
                    )
                })
                .collect(),
        }
    } else {
        serde_json::from_slice::<KnowledgeState>(&bytes)
            .map_err(|_| WenlanError::Conflict("page_projection_state_invalid".into()))?
    };
    let mut state = state;
    if state.schema_version == 0 {
        state.schema_version = KNOWLEDGE_STATE_SCHEMA_V2;
    }
    if state.schema_version != KNOWLEDGE_STATE_SCHEMA_V2 {
        return Err(WenlanError::Conflict(
            "page_projection_state_invalid".into(),
        ));
    }
    let mut paths = std::collections::HashSet::new();
    for entry in state.pages.values() {
        validate_page_path(&entry.file)?;
        if !paths.insert(collision_key(&entry.file)) {
            return Err(WenlanError::Conflict(
                "page_projection_state_invalid".into(),
            ));
        }
    }
    Ok(state)
}

fn save_state(control: &Dir, state: &KnowledgeState) -> Result<(), WenlanError> {
    let temporary = format!(
        ".projection-state-{}.{}.tmp",
        std::process::id(),
        uuid::Uuid::new_v4()
    );
    write_page_atomically_nofollow(
        control,
        "state.json",
        &temporary,
        &serde_json::to_vec_pretty(state)?,
    )?;
    sync_dir_capability(control)
}

pub(super) fn read_root(path: &Path) -> Result<Dir, WenlanError> {
    let parent = path.parent().ok_or_else(invalid_path)?;
    let name = path.file_name().ok_or_else(invalid_path)?;
    let parent = Dir::open_ambient_dir(parent, cap_std::ambient_authority())?;
    Ok(parent.open_dir_nofollow(Path::new(name))?)
}

pub fn list_knowledge_folders(path: &Path) -> Result<Vec<KnowledgeFolderEntry>, WenlanError> {
    let root = match read_root(path) {
        Ok(root) => root,
        Err(WenlanError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Vec::new())
        }
        Err(error) => return Err(error),
    };
    Ok(scan_tree(&root)?.0)
}

/// Bounded, nofollow discovery shared by daemon watcher and offline CLI reader.
pub fn knowledge_markdown_paths(path: &Path) -> Result<Vec<String>, WenlanError> {
    match read_root(path) {
        Ok(root) => Ok(scan_tree(&root)?.1),
        Err(WenlanError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(Vec::new())
        }
        Err(error) => Err(error),
    }
}

pub fn read_knowledge_markdown(
    path: &Path,
    relative: &str,
) -> Result<(String, std::time::SystemTime), WenlanError> {
    let root = read_root(path)?;
    let (parent, name) = page_parent(&root, relative)?;
    let mut file = parent.open_with(&name, &regular_read_options())?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(invalid_path());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(REPAIR_CAPABILITY_READ_MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > REPAIR_CAPABILITY_READ_MAX_BYTES {
        return Err(repair_projection_rollback_too_large());
    }
    let modified = metadata.modified()?.into_std();
    Ok((
        String::from_utf8(bytes).map_err(|_| invalid_path())?,
        modified,
    ))
}

/// Recover before taking a watcher snapshot, then identify only unambiguous mappings.
pub fn knowledge_edit_targets(path: &Path) -> Result<HashMap<String, String>, WenlanError> {
    KnowledgeProjectionWrite::with_projection_capabilities(path, |capabilities| {
        let state = read_state_strict(&capabilities.wenlan)?;
        let mut claims: HashMap<String, Vec<String>> = HashMap::new();
        for path in scan_tree(&capabilities.root)?.1 {
            if is_reserved_okf_filename(path.rsplit('/').next().unwrap_or(&path)) {
                continue;
            }
            let (parent, name) = page_parent(&capabilities.root, &path)?;
            if let Some(id) = KnowledgeWriter::read_origin_id(&parent, &OsString::from(name)) {
                claims.entry(id).or_default().push(path);
            }
        }
        Ok(state
            .pages
            .into_iter()
            .filter_map(|(id, entry)| {
                (claims
                    .get(&id)
                    .is_some_and(|paths| paths.len() == 1 && paths[0] == entry.file))
                .then_some((id, entry.file))
            })
            .collect())
    })
}

pub fn create_knowledge_folder(
    path: &Path,
    parent_path: &str,
    name: &str,
) -> Result<String, WenlanError> {
    validate_knowledge_folder_path(parent_path)?;
    validate_component(name)?;
    if reserved_directory(name) {
        return Err(invalid_path());
    }
    let created_path = if parent_path.is_empty() {
        name.to_string()
    } else {
        format!("{parent_path}/{name}")
    };
    validate_knowledge_folder_path(&created_path)?;
    create_projection_root_nofollow(path)?;
    KnowledgeProjectionWrite::with_projection_capabilities(path, |capabilities| {
        read_state_strict(&capabilities.wenlan)?;
        let parent = open_folder(&capabilities.root, parent_path)?;
        ensure_case_available(&parent, name, true)?;
        match parent.symlink_metadata(name) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => return Err(WenlanError::Conflict("knowledge_path_collision".into())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                parent.create_dir(name)?;
            }
            Err(error) => return Err(error.into()),
        }
        // Check the created object too; an external writer may have raced mkdir.
        parent.open_dir_nofollow(name)?;
        sync_dir_capability(&parent)?;
        Ok(created_path)
    })
}

#[derive(Serialize, Deserialize)]
struct MoveJournal {
    version: u32,
    operation_id: String,
    page_id: String,
    source: String,
    destination: String,
    stage: String,
    content_sha256: String,
    entry_sha256: String,
    device: u64,
    inode: u64,
}

fn recovery_required() -> WenlanError {
    WenlanError::Conflict("page_move_recovery_required".into())
}

fn file_snapshot(
    directory: &Dir,
    name: &str,
) -> Result<Option<(String, ProjectionFileIdentity)>, WenlanError> {
    let mut budget = RepairReadBudget::new();
    match read_regular_identity_nofollow_bounded(directory, OsStr::new(name), &mut budget) {
        Ok((bytes, identity)) => Ok(Some((projection_digest(&bytes), identity))),
        Err(WenlanError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn matches_snapshot(
    snapshot: &Option<(String, ProjectionFileIdentity)>,
    journal: &MoveJournal,
) -> bool {
    snapshot.as_ref().is_some_and(|(digest, identity)| {
        digest == &journal.content_sha256
            && identity.device == journal.device
            && identity.inode == journal.inode
    })
}

fn verify_parent_identity(root: &Dir, relative: &str, pinned: &Dir) -> Result<(), WenlanError> {
    let folder = relative.rsplit_once('/').map_or("", |(folder, _)| folder);
    let current = open_folder(root, folder)?;
    if projection_file_identity(&current.dir_metadata()?)?
        != projection_file_identity(&pinned.dir_metadata()?)?
    {
        return Err(recovery_required());
    }
    Ok(())
}

/// Recover only exact journal-owned bytes/paths. Ambiguity preserves every inode.
pub(super) fn recover_move(capabilities: &ProjectionCapabilities) -> Result<(), WenlanError> {
    recover_move_with_checkpoint(capabilities, &mut |_| Ok(()))
}

fn recover_move_with_checkpoint(
    capabilities: &ProjectionCapabilities,
    checkpoint: &mut impl FnMut(&str) -> Result<(), WenlanError>,
) -> Result<(), WenlanError> {
    let mut budget = RepairReadBudget::new();
    let Some(bytes) = read_optional_regular_nofollow(
        &capabilities.wenlan,
        OsStr::new(MOVE_JOURNAL),
        &mut budget,
    )?
    else {
        return Ok(());
    };
    let journal: MoveJournal = serde_json::from_slice(&bytes).map_err(|_| recovery_required())?;
    if journal.version != 1
        || journal
            .stage
            .strip_prefix(".page-move-stage-")
            .and_then(|suffix| uuid::Uuid::parse_str(suffix).ok())
            .is_none()
    {
        return Err(recovery_required());
    }
    validate_page_path(&journal.source)?;
    validate_page_path(&journal.destination)?;
    let (source_dir, source_name) = page_parent(&capabilities.root, &journal.source)?;
    let (target_dir, target_name) = page_parent(&capabilities.root, &journal.destination)?;
    let source = file_snapshot(&source_dir, &source_name)?;
    let stage = file_snapshot(&capabilities.wenlan, &journal.stage)?;
    let target = file_snapshot(&target_dir, &target_name)?;
    let mut state = read_state_strict(&capabilities.wenlan)?;
    let entry = state
        .pages
        .get_mut(&journal.page_id)
        .ok_or_else(recovery_required)?;
    if entry.file != journal.source && entry.file != journal.destination {
        return Err(recovery_required());
    }
    let mut before_entry = entry.clone();
    before_entry.file = journal.source.clone();
    if projection_digest(&serde_json::to_vec(&before_entry)?) != journal.entry_sha256 {
        return Err(recovery_required());
    }
    if stage.is_none()
        && target.is_none()
        && matches_snapshot(&source, &journal)
        && entry.file == journal.source
    {
        capabilities.wenlan.remove_file(MOVE_JOURNAL)?;
        sync_dir_capability(&capabilities.wenlan)?;
        return Ok(());
    }
    if source.is_some()
        || (!matches_snapshot(&stage, &journal) && !matches_snapshot(&target, &journal))
        || target
            .as_ref()
            .is_some_and(|_| !matches_snapshot(&target, &journal))
        || stage
            .as_ref()
            .is_some_and(|_| !matches_snapshot(&stage, &journal))
    {
        return Err(recovery_required());
    }
    if target.is_none() {
        verify_parent_identity(&capabilities.root, &journal.destination, &target_dir)?;
        ensure_case_available(&target_dir, &target_name, false)?;
        capabilities
            .wenlan
            .hard_link(&journal.stage, &target_dir, &target_name)?;
        sync_dir_capability(&target_dir)?;
    }
    checkpoint("installed")?;
    verify_parent_identity(&capabilities.root, &journal.destination, &target_dir)?;
    verify_parent_identity(&capabilities.root, &journal.source, &source_dir)?;
    // Origin and inode are checked after installation; never overwrite a raced target.
    if KnowledgeWriter::read_origin_id(&target_dir, &OsString::from(&target_name)).as_deref()
        != Some(&journal.page_id)
        || !matches_snapshot(&file_snapshot(&target_dir, &target_name)?, &journal)
    {
        return Err(recovery_required());
    }
    if file_snapshot(&source_dir, &source_name)?.is_some() {
        return Err(recovery_required());
    }
    entry.file = journal.destination.clone();
    save_state(&capabilities.wenlan, &state)?;
    checkpoint("state")?;
    if file_snapshot(&source_dir, &source_name)?.is_some() {
        return Err(recovery_required());
    }
    if stage.is_some() {
        if !matches_snapshot(
            &file_snapshot(&capabilities.wenlan, &journal.stage)?,
            &journal,
        ) {
            return Err(recovery_required());
        }
        capabilities.wenlan.remove_file(&journal.stage)?;
    }
    capabilities.wenlan.remove_file(MOVE_JOURNAL)?;
    sync_dir_capability(&capabilities.wenlan)?;
    if let Err(error) = KnowledgeWriter::regenerate_index_cap(capabilities, &state) {
        log::warn!("[knowledge] moved page but index regeneration failed: {error}");
    }
    Ok(())
}

pub fn move_projected_page(
    path: &Path,
    page_id: &str,
    expected_storage_path: &str,
    folder_path: &str,
    operation_id: &str,
) -> Result<String, WenlanError> {
    move_projected_page_with_checkpoint(
        path,
        page_id,
        expected_storage_path,
        folder_path,
        operation_id,
        &mut |_| Ok(()),
    )
}

fn move_projected_page_with_checkpoint(
    path: &Path,
    page_id: &str,
    expected_storage_path: &str,
    folder_path: &str,
    operation_id: &str,
    checkpoint: &mut impl FnMut(&str) -> Result<(), WenlanError>,
) -> Result<String, WenlanError> {
    validate_page_path(expected_storage_path)?;
    validate_knowledge_folder_path(folder_path)?;
    if operation_id.is_empty()
        || operation_id.len() > 128
        || operation_id.chars().any(char::is_control)
    {
        return Err(invalid_path());
    }
    KnowledgeProjectionWrite::with_projection_capabilities(path, |capabilities| {
        let state = read_state_strict(&capabilities.wenlan)?;
        let entry = state
            .pages
            .get(page_id)
            .ok_or_else(|| WenlanError::Conflict("page_projection_missing".into()))?;
        let name = expected_storage_path
            .rsplit('/')
            .next()
            .ok_or_else(invalid_path)?;
        if is_reserved_okf_filename(name) {
            return Err(invalid_path());
        }
        let destination = if folder_path.is_empty() {
            name.to_string()
        } else {
            format!("{folder_path}/{name}")
        };
        let (target_dir, target_name) = page_parent(&capabilities.root, &destination)?;
        if entry.file == destination {
            ensure_case_available(&target_dir, &target_name, true)?;
            if origin_paths(&capabilities.root, page_id)? == [destination.clone()]
                && KnowledgeWriter::read_origin_id(&target_dir, &OsString::from(&target_name))
                    .as_deref()
                    == Some(page_id)
            {
                return Ok(destination);
            }
            return Err(recovery_required());
        }
        if entry.file != expected_storage_path {
            return Err(WenlanError::Conflict("page_storage_path_changed".into()));
        }
        ensure_case_available(&target_dir, &target_name, false)?;
        let claimants = origin_paths(&capabilities.root, page_id)?;
        if claimants != [expected_storage_path] {
            return Err(WenlanError::Conflict(
                "page_projection_identity_ambiguous".into(),
            ));
        }
        let (source_dir, source_name) = page_parent(&capabilities.root, expected_storage_path)?;
        let (digest, identity) =
            file_snapshot(&source_dir, &source_name)?.ok_or_else(recovery_required)?;
        // A nested mount point must not strand a source halfway through rename/link.
        if projection_file_identity(&capabilities.wenlan.dir_metadata()?)?.device != identity.device
            || projection_file_identity(&target_dir.dir_metadata()?)?.device != identity.device
        {
            return Err(WenlanError::Conflict("page_move_cross_device".into()));
        }
        let journal = MoveJournal {
            version: 1,
            operation_id: operation_id.to_string(),
            page_id: page_id.to_string(),
            source: expected_storage_path.to_string(),
            destination: destination.clone(),
            stage: format!(".page-move-stage-{}", uuid::Uuid::new_v4()),
            content_sha256: digest,
            entry_sha256: projection_digest(&serde_json::to_vec(entry)?),
            device: identity.device,
            inode: identity.inode,
        };
        write_new_file_nofollow(
            &capabilities.wenlan,
            OsStr::new(MOVE_JOURNAL),
            &serde_json::to_vec(&journal)?,
        )?;
        sync_dir_capability(&capabilities.wenlan)?;
        checkpoint("journal")?;
        verify_parent_identity(&capabilities.root, expected_storage_path, &source_dir)?;
        verify_parent_identity(&capabilities.root, &destination, &target_dir)?;
        source_dir.rename(&source_name, &capabilities.wenlan, &journal.stage)?;
        sync_dir_capability(&source_dir)?;
        sync_dir_capability(&capabilities.wenlan)?;
        checkpoint("stage")?;
        recover_move_with_checkpoint(capabilities, checkpoint)?;
        Ok(destination)
    })
}

pub(super) fn origin_paths(root: &Dir, page_id: &str) -> Result<Vec<String>, WenlanError> {
    let mut paths = Vec::new();
    for path in scan_tree(root)?.1 {
        // A user's index/log made by copying a Page is still a reserved navigation
        // document, not a second writable Page. Preserve its existing protection.
        if is_reserved_okf_filename(path.rsplit('/').next().unwrap_or(&path)) {
            continue;
        }
        let (parent, name) = page_parent(root, &path)?;
        if KnowledgeWriter::read_origin_id(&parent, &OsString::from(name)).as_deref()
            == Some(page_id)
        {
            paths.push(path);
        }
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded() -> (tempfile::TempDir, KnowledgeWriter, Page, String) {
        let dir = tempfile::tempdir().unwrap();
        let writer = KnowledgeWriter::new_for_test(dir.path().to_path_buf());
        let page = super::super::tests::test_concept();
        writer.write_page_for_test(&page).unwrap();
        let source = writer.page_filename(&page.id).unwrap();
        (dir, writer, page, source)
    }

    #[test]
    fn physical_nested_folders_are_idempotent_and_exclude_controls() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            create_knowledge_folder(dir.path(), "", "Research").unwrap(),
            "Research"
        );
        assert_eq!(
            create_knowledge_folder(dir.path(), "Research", "Rust").unwrap(),
            "Research/Rust"
        );
        create_knowledge_folder(dir.path(), "Research", "Rust").unwrap();
        std::fs::create_dir(dir.path().join("archive")).unwrap();
        std::fs::create_dir(dir.path().join("_sources")).unwrap();
        assert_eq!(
            list_knowledge_folders(dir.path())
                .unwrap()
                .iter()
                .map(|entry| entry.path.as_str())
                .collect::<Vec<_>>(),
            ["Research", "Research/Rust"]
        );
        assert!(create_knowledge_folder(dir.path(), "", "research").is_err());
    }

    #[test]
    fn portable_paths_fail_closed() {
        for invalid in [
            "/root",
            "../root",
            "a//b",
            "a/./b",
            "a/../b",
            "C:root",
            "a\\b",
            "a/",
            ".wenlan",
            "a/archive",
            "_sources",
            "CON",
            "aux.txt",
            "LPT1",
            "a.",
            "a ",
            "a\0b",
            "a*b",
        ] {
            assert!(
                validate_knowledge_folder_path(invalid).is_err(),
                "accepted {invalid:?}"
            );
        }
        assert_eq!(
            validate_knowledge_folder_path("研究/Rust").unwrap(),
            "研究/Rust"
        );
        assert_eq!(validate_knowledge_folder_path("").unwrap(), "");
    }

    #[test]
    fn first_placement_and_future_saves_keep_the_recorded_folder() {
        let dir = tempfile::tempdir().unwrap();
        let writer = KnowledgeWriter::new_for_test(dir.path().to_path_buf());
        create_knowledge_folder(dir.path(), "", "Notes").unwrap();
        create_knowledge_folder(dir.path(), "Notes", "Rust").unwrap();
        let mut page = super::super::tests::test_concept();
        let guard = writer.begin_test_write();
        writer
            .write_page_in_folder(&guard, &page, Some("Notes/Rust"))
            .unwrap();
        let path = writer.page_filename(&page.id).unwrap();
        assert_eq!(path, "Notes/Rust/rust-ownership.md");
        page.version += 1;
        page.content.push_str("\nUpdated");
        writer
            .write_page_in_folder(&guard, &page, Some(""))
            .unwrap();
        assert_eq!(
            writer.page_filename(&page.id).as_deref(),
            Some(path.as_str())
        );
        assert!(std::fs::read_to_string(dir.path().join(&path))
            .unwrap()
            .contains("Updated"));
        assert!(!dir.path().join("rust-ownership.md").exists());
        assert_eq!(writer.live_page_filenames(&[&page.id])[&page.id], path);
        assert!(std::fs::read_to_string(dir.path().join("index.md"))
            .unwrap()
            .contains("/Notes/Rust/rust-ownership.md"));
    }

    #[cfg(unix)]
    #[test]
    fn legacy_local_paths_survive_save_discovery_and_unrelated_writes() {
        for legacy in ["con.md", ".md", "Project: notes/con.md"] {
            let (dir, writer, mut page, source) = seeded();
            std::fs::create_dir_all(dir.path().join("Project: notes")).unwrap();
            std::fs::write(
                dir.path().join("Project: notes/local.md"),
                b"local user prose",
            )
            .unwrap();
            std::fs::rename(dir.path().join(&source), dir.path().join(legacy)).unwrap();
            let mut state = writer.load_state();
            state.pages.get_mut(&page.id).unwrap().file = legacy.into();
            writer.save_state(&state).unwrap();
            let before = std::fs::read(dir.path().join(legacy)).unwrap();
            writer.write_page_for_test(&page).unwrap();
            assert_eq!(std::fs::read(dir.path().join(legacy)).unwrap(), before);
            assert_eq!(writer.live_page_filenames(&[&page.id])[&page.id], legacy);
            assert!(knowledge_markdown_paths(dir.path())
                .unwrap()
                .contains(&legacy.into()));
            assert!(list_knowledge_folders(dir.path())
                .unwrap()
                .iter()
                .any(|f| f.path == "Project: notes"));
            let mut unrelated = page.clone();
            unrelated.id = "page_unrelated".into();
            unrelated.title = "Unrelated note".into();
            writer.write_page_for_test(&unrelated).unwrap();
            create_knowledge_folder(dir.path(), "", "New").unwrap();
            page.version += 1;
            page.content.push_str("\nLegacy location updated");
            writer.write_page_for_test(&page).unwrap();
            assert_eq!(writer.page_filename(&page.id).as_deref(), Some(legacy));
            assert!(std::fs::read_to_string(dir.path().join(legacy))
                .unwrap()
                .contains("Legacy location updated"));
            assert_eq!(
                std::fs::read(dir.path().join("Project: notes/local.md")).unwrap(),
                b"local user prose"
            );
            assert!(create_knowledge_folder(dir.path(), "", "Project: another").is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn hidden_and_nonportable_origin_copies_are_found_and_evicted() {
        let (dir, writer, page, source) = seeded();
        std::fs::create_dir(dir.path().join("Project: notes")).unwrap();
        for copy in [".copy.md", "Project: notes/con.md"] {
            std::fs::copy(dir.path().join(&source), dir.path().join(copy)).unwrap();
        }
        let bytes = std::fs::read(dir.path().join(&source)).unwrap();
        assert_eq!(writer.projected_files().unwrap()[&page.id].len(), 3);
        assert!(writer.write_page_for_test(&page).is_err());
        let guard = writer.begin_test_write();
        assert_eq!(
            writer
                .evict_projected_pages(&guard, std::slice::from_ref(&page.id))
                .unwrap(),
            1
        );
        for removed in [source.as_str(), ".copy.md", "Project: notes/con.md"] {
            assert!(!dir.path().join(removed).exists());
        }
        for archived in [source.as_str(), ".copy.md", "con.md"] {
            assert_eq!(
                std::fs::read(dir.path().join("archive").join(archived)).unwrap(),
                bytes
            );
        }
    }

    #[test]
    fn encoded_folder_index_links_stay_owned_and_update_after_move_and_save() {
        for (folder, encoded) in [
            ("Work (2026)", "Work%20%282026%29"),
            ("#", "%23"),
            ("%", "%25"),
        ] {
            let (dir, writer, mut page, source) = seeded();
            create_knowledge_folder(dir.path(), "", folder).unwrap();
            let moved =
                move_projected_page(dir.path(), &page.id, &source, folder, "index-encoding")
                    .unwrap();
            assert_eq!(moved, format!("{folder}/{source}"));
            let index = std::fs::read_to_string(dir.path().join("index.md")).unwrap();
            assert!(index.contains(&format!("(/{encoded}/{source})")), "{index}");
            assert!(index_body_is_generated(
                index.split_once("---\n\n").unwrap().1
            ));
            let target = format!("/{encoded}/{source}");
            let mut decoded = Vec::new();
            let mut bytes = target.as_bytes().iter().copied();
            while let Some(byte) = bytes.next() {
                decoded.push(if byte == b'%' {
                    let pair = [bytes.next().unwrap(), bytes.next().unwrap()];
                    u8::from_str_radix(std::str::from_utf8(&pair).unwrap(), 16).unwrap()
                } else {
                    byte
                });
            }
            assert_eq!(String::from_utf8(decoded).unwrap(), format!("/{moved}"));
            page.title = "Updated index title".into();
            page.version += 1;
            writer.write_page_for_test(&page).unwrap();
            let index = std::fs::read_to_string(dir.path().join("index.md")).unwrap();
            assert!(
                index.contains(&format!("[Updated index title](/{encoded}/{source})")),
                "{index}"
            );
            assert_eq!(
                writer.page_filename(&page.id).as_deref(),
                Some(moved.as_str())
            );
            let mut other = page.clone();
            other.id = "page_another".into();
            other.title = "Another title".into();
            writer
                .write_page_in_folder(&writer.begin_test_write(), &other, Some(folder))
                .unwrap();
            let index = std::fs::read_to_string(dir.path().join("index.md")).unwrap();
            assert!(
                index.contains(&format!("[Another title](/{encoded}/another-title.md)")),
                "{index}"
            );
        }
    }

    #[test]
    fn title_slugs_allocate_visible_portable_filenames() {
        for (title, expected) in [
            ("📒", "note.md"),
            ("CON", "con-page.md"),
            ("COM¹", "com¹-page.md"),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let writer = KnowledgeWriter::new_for_test(dir.path().to_path_buf());
            let mut page = super::super::tests::test_concept();
            page.title = title.into();
            writer.write_page_for_test(&page).unwrap();
            assert_eq!(writer.page_filename(&page.id).as_deref(), Some(expected));
        }
    }

    #[test]
    fn moving_preserves_external_prose_and_identity_without_re_rendering() {
        let (dir, writer, page, source) = seeded();
        create_knowledge_folder(dir.path(), "", "Notes").unwrap();
        let raw = std::fs::read_to_string(dir.path().join(&source))
            .unwrap()
            .replace("Rust uses ownership", "My external prose uses ownership");
        std::fs::write(dir.path().join(&source), &raw).unwrap();
        let target = move_projected_page(dir.path(), &page.id, &source, "Notes", "move-1").unwrap();
        assert_eq!(target, format!("Notes/{source}"));
        assert_eq!(
            std::fs::read_to_string(dir.path().join(&target)).unwrap(),
            raw
        );
        assert!(!dir.path().join(&source).exists());
        assert_eq!(writer.page_filename(&page.id).unwrap(), target);
        assert!(
            writer.write_page_for_test(&page).is_err(),
            "external prose must stay protected until watcher adoption"
        );
        assert_eq!(
            move_projected_page(dir.path(), &page.id, &source, "Notes", "move-1").unwrap(),
            target
        );
    }

    #[test]
    fn nested_reconcile_removal_and_truth_eviction_cover_every_copy() {
        let (dir, writer, mut page, source) = seeded();
        create_knowledge_folder(dir.path(), "", "Notes").unwrap();
        let target =
            move_projected_page(dir.path(), &page.id, &source, "Notes", "nested-lifecycle")
                .unwrap();
        page.version += 1;
        page.content.push_str("\nNewer canonical content");
        let stats = writer.reconcile_for_test(&[page.clone()]).unwrap();
        assert_eq!(stats.rewritten, 1);
        assert!(std::fs::read_to_string(dir.path().join(&target))
            .unwrap()
            .contains("Newer canonical content"));
        std::fs::copy(
            dir.path().join(&target),
            dir.path().join("Notes/conflict-copy.md"),
        )
        .unwrap();
        assert_eq!(writer.projected_files().unwrap()[&page.id].len(), 2);
        let before = std::fs::read(dir.path().join(&target)).unwrap();
        let guard = writer.begin_test_write();
        assert_eq!(
            writer
                .evict_projected_pages(&guard, &[page.id.clone()])
                .unwrap(),
            1
        );
        assert!(!dir.path().join(&target).exists());
        assert!(!dir.path().join("Notes/conflict-copy.md").exists());
        assert!(!writer.load_state().pages.contains_key(&page.id));
        assert_eq!(
            std::fs::read(dir.path().join("archive").join(&source)).unwrap(),
            before
        );
        assert_eq!(
            std::fs::read(dir.path().join("archive/conflict-copy.md")).unwrap(),
            before
        );
        assert!(knowledge_markdown_paths(dir.path())
            .unwrap()
            .iter()
            .all(|path| !path.starts_with("archive/")));

        writer
            .write_page_in_folder(&guard, &page, Some("Notes"))
            .unwrap();
        writer.remove_page(&guard, &page.id).unwrap();
        assert!(!dir.path().join(&target).exists());
    }

    #[test]
    fn move_destination_collision_keeps_both_files() {
        let (dir, _writer, page, source) = seeded();
        create_knowledge_folder(dir.path(), "", "Notes").unwrap();
        let before = std::fs::read(dir.path().join(&source)).unwrap();
        std::fs::write(
            dir.path().join("Notes").join(&source),
            b"existing user note",
        )
        .unwrap();
        assert!(move_projected_page(dir.path(), &page.id, &source, "Notes", "collision").is_err());
        assert_eq!(std::fs::read(dir.path().join(&source)).unwrap(), before);
        assert_eq!(
            std::fs::read(dir.path().join("Notes").join(&source)).unwrap(),
            b"existing user note"
        );
        assert!(!dir.path().join(".wenlan").join(MOVE_JOURNAL).exists());
    }

    #[test]
    fn every_durable_move_edge_recovers_before_an_ordinary_write() {
        for edge in ["journal", "stage", "installed", "state"] {
            let (dir, writer, page, source) = seeded();
            create_knowledge_folder(dir.path(), "", "Notes").unwrap();
            let mut interrupt = |checkpoint: &str| {
                if checkpoint == edge {
                    Err(WenlanError::Conflict("simulated_interrupt".into()))
                } else {
                    Ok(())
                }
            };
            assert!(move_projected_page_with_checkpoint(
                dir.path(),
                &page.id,
                &source,
                "Notes",
                edge,
                &mut interrupt
            )
            .is_err());
            writer.write_page_for_test(&page).unwrap();
            let expected = if edge == "journal" {
                source.clone()
            } else {
                format!("Notes/{source}")
            };
            assert_eq!(writer.page_filename(&page.id).unwrap(), expected);
            assert_eq!(
                knowledge_markdown_paths(dir.path())
                    .unwrap()
                    .into_iter()
                    .filter(|path| path.ends_with(&source))
                    .collect::<Vec<_>>(),
                [expected]
            );
            assert!(!dir.path().join(".wenlan").join(MOVE_JOURNAL).exists());
        }
    }

    #[test]
    fn raced_destination_or_source_never_gets_overwritten_or_deleted() {
        for edge in ["stage", "installed"] {
            let (dir, writer, page, source) = seeded();
            create_knowledge_folder(dir.path(), "", "Notes").unwrap();
            let before = std::fs::read(dir.path().join(&source)).unwrap();
            let target = format!("Notes/{source}");
            let raced = if edge == "stage" { &target } else { &source };
            let mut race = |checkpoint: &str| {
                if checkpoint == edge {
                    std::fs::write(dir.path().join(raced), b"external racing save")?;
                }
                Ok(())
            };
            assert!(move_projected_page_with_checkpoint(
                dir.path(),
                &page.id,
                &source,
                "Notes",
                edge,
                &mut race
            )
            .is_err());
            assert_eq!(
                std::fs::read(dir.path().join(raced)).unwrap(),
                b"external racing save"
            );
            let journal: MoveJournal = serde_json::from_slice(
                &std::fs::read(dir.path().join(".wenlan").join(MOVE_JOURNAL)).unwrap(),
            )
            .unwrap();
            assert_eq!(
                std::fs::read(dir.path().join(".wenlan").join(&journal.stage)).unwrap(),
                before
            );
            assert!(writer.write_page_for_test(&page).is_err());
            assert_eq!(
                std::fs::read(dir.path().join(raced)).unwrap(),
                b"external racing save"
            );
        }
    }

    #[test]
    fn duplicate_origin_external_move_and_corrupt_state_block_mutation() {
        let (dir, writer, page, source) = seeded();
        create_knowledge_folder(dir.path(), "", "Notes").unwrap();
        std::fs::copy(dir.path().join(&source), dir.path().join("Notes/copy.md")).unwrap();
        assert!(writer.write_page_for_test(&page).is_err());
        assert!(move_projected_page(dir.path(), &page.id, &source, "Notes", "duplicate").is_err());
        std::fs::remove_file(dir.path().join(&source)).unwrap();
        assert!(
            writer.write_page_for_test(&page).is_err(),
            "an external move must not recreate the old path"
        );
        std::fs::write(dir.path().join(".wenlan/state.json"), b"{corrupt").unwrap();
        assert!(create_knowledge_folder(dir.path(), "", "New").is_err());
        assert!(writer.write_page_for_test(&page).is_err());
        assert_eq!(
            std::fs::read(dir.path().join(".wenlan/state.json")).unwrap(),
            b"{corrupt"
        );
    }

    #[test]
    fn duplicate_state_members_are_rejected_without_rewriting_state() {
        let (dir, writer, page, _source) = seeded();
        let entry = serde_json::to_string(&writer.load_state().pages[&page.id]).unwrap();
        let raw = format!(
            r#"{{"schema_version":2,"pages":{{"{}":{},"{}":{}}}}}"#,
            page.id, entry, page.id, entry
        );
        std::fs::write(dir.path().join(".wenlan/state.json"), &raw).unwrap();
        assert!(writer.write_page_for_test(&page).is_err());
        assert_eq!(
            std::fs::read_to_string(dir.path().join(".wenlan/state.json")).unwrap(),
            raw
        );
    }

    #[test]
    fn an_editor_writing_the_open_moved_inode_leaves_its_latest_bytes_recoverable() {
        let (dir, writer, page, source) = seeded();
        create_knowledge_folder(dir.path(), "", "Notes").unwrap();
        let mut race = |checkpoint: &str| {
            if checkpoint == "stage" {
                let journal: MoveJournal = serde_json::from_slice(&std::fs::read(
                    dir.path().join(".wenlan").join(MOVE_JOURNAL),
                )?)
                .unwrap();
                std::fs::write(
                    dir.path().join(".wenlan").join(journal.stage),
                    b"latest external editor bytes",
                )?;
            }
            Ok(())
        };
        assert!(move_projected_page_with_checkpoint(
            dir.path(),
            &page.id,
            &source,
            "Notes",
            "open-editor",
            &mut race
        )
        .is_err());
        let journal: MoveJournal = serde_json::from_slice(
            &std::fs::read(dir.path().join(".wenlan").join(MOVE_JOURNAL)).unwrap(),
        )
        .unwrap();
        assert_eq!(
            std::fs::read(dir.path().join(".wenlan").join(journal.stage)).unwrap(),
            b"latest external editor bytes"
        );
        assert!(writer.write_page_for_test(&page).is_err());
        assert!(!dir.path().join("Notes").join(&source).exists());
    }

    #[cfg(unix)]
    #[test]
    fn nofollow_nested_symlink_never_reads_or_writes_outside_root() {
        let (dir, _writer, page, source) = seeded();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("outside.md"), b"private outside data").unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("Linked")).unwrap();
        assert!(list_knowledge_folders(dir.path()).unwrap().is_empty());
        assert!(create_knowledge_folder(dir.path(), "Linked", "New").is_err());
        assert!(move_projected_page(dir.path(), &page.id, &source, "Linked", "symlink").is_err());
        assert!(read_knowledge_markdown(dir.path(), "Linked/outside.md").is_err());
        assert_eq!(
            std::fs::read(outside.path().join("outside.md")).unwrap(),
            b"private outside data"
        );
    }
}
