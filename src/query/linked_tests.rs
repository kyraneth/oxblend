use super::*;
#[test]
fn lexical_aliases_have_independent_requests_and_shared_canonical_provenance() {
    let Some(root) = std::env::var_os("OXBLEND_QUERY_LINKED_FIXTURE").map(PathBuf::from) else {
        eprintln!("native linked fixture not selected");
        return;
    };
    let root_reader = BlendFile::open(&root).unwrap();
    let library = root.parent().unwrap().join("group-library.blend");
    let directory = std::env::temp_dir().join(format!(
        "oxblend-query-aliases-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let a = directory.join("A.blend");
    let b = directory.join("B.blend");
    std::os::unix::fs::symlink(&library, &a).unwrap();
    std::os::unix::fs::symlink(&library, &b).unwrap();
    let original = json!([{"kind":"NodeTree","name":"SharedTree","library":a},{"kind":"NodeTree","name":"SharedTree","library":b}]);
    let empty = LibraryMap::new();
    let mut plan = Context::new(&root_reader, None, &empty);
    let mut value = original.clone();
    assert_eq!(plan.enrich(&mut value).len(), 2);
    let requests = plan.requests();
    assert_eq!(requests.len(), 2);
    assert_ne!(requests[0].key, requests[1].key);
    assert_eq!(
        requests[0].path.canonicalize().unwrap(),
        requests[1].path.canonicalize().unwrap()
    );
    let mut captures: BTreeMap<PathBuf, BlendFile> = BTreeMap::new();
    for request in &requests {
        assert!(
            captures
                .insert(request.key.clone(), BlendFile::open(&request.path).unwrap())
                .is_none()
        );
    }
    let views: LibraryMap<'_> = captures
        .iter()
        .map(|(key, reader)| {
            (
                key.clone(),
                Ok(LibraryView {
                    reader,
                    provenance: json!({"owner":"alias-fixture"}),
                }),
            )
        })
        .collect();
    let mut context = Context::new(&root_reader, None, &views);
    let mut value = original;
    assert!(context.enrich(&mut value).is_empty());
    assert!(context.requests().is_empty());
    let actual = context
        .observations
        .iter()
        .filter(|o| o["selected_from"] == "library_local")
        .collect::<Vec<_>>();
    assert_eq!(actual.len(), 2);
    assert_ne!(actual[0]["library_path"], actual[1]["library_path"]);
    assert_eq!(
        actual[0]["canonical_identity"],
        actual[1]["canonical_identity"]
    );
    assert_eq!(value[0]["kind"], "CompositorNodeTree");
    assert_eq!(value[1]["kind"], "CompositorNodeTree");
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn relative_root_context_reuses_existing_reader_and_retains_failed_captures() {
    let Some(root) = std::env::var_os("OXBLEND_QUERY_LINKED_FIXTURE").map(PathBuf::from) else {
        return;
    };
    let library = absolute_lexical(&root.parent().unwrap().join("group-library.blend")).unwrap();
    let reader = BlendFile::open(&library).unwrap();
    let mut relative = PathBuf::new();
    for component in std::env::current_dir().unwrap().components() {
        if matches!(component, std::path::Component::Normal(_)) {
            relative.push("..");
        }
    }
    for component in library.components() {
        if let std::path::Component::Normal(name) = component {
            relative.push(name);
        }
    }
    assert!(relative.is_relative());
    let empty = LibraryMap::new();
    let mut context = Context::new(&reader, Some(&relative), &empty);
    let actual = context
        .resolve_id(
            Some(&relative),
            library.as_os_str().as_encoded_bytes(),
            b"NTSharedTree",
            "root_reuse_test",
        )
        .unwrap();
    assert!(std::ptr::eq(actual.reader, &reader));
    assert_eq!(actual.path, library);
    assert!(context.requests().is_empty());
    let mut failed = LibraryMap::new();
    failed.insert(
        library.clone(),
        Err(Error::new("library_read", "latched fixture failure")),
    );
    let mut context = Context::new(&reader, Some(&relative), &failed);
    let error = context
        .resolve_id(
            Some(&relative),
            library.as_os_str().as_encoded_bytes(),
            b"NTSharedTree",
            "root_reuse_test",
        )
        .err()
        .unwrap();
    assert_eq!(error.kind, "library_read");
    assert!(context.requests().is_empty());
}
