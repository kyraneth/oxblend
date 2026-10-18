//! Native-fixture additive API/snapshot binding probe. no original writes.
use oxblend::edit::EditOptions;
use oxblend::typed_edit::{EditMode, TypedValue, TypedWrite, plan_fields, snapshot, write_plan};
use oxblend::value::{PathStep, Value};
use std::path::Path;
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let input = Path::new(&args[1]);
    let output = Path::new(&args[2]);
    let first = snapshot(input).unwrap();
    let second = snapshot(input).unwrap();
    let block = first
        .reader()
        .blocks()
        .iter()
        .position(|b| b.code == *b"SC\0\0")
        .unwrap();
    let path = || {
        vec![
            PathStep::Field(b"r".to_vec()),
            PathStep::Field(b"sfra".to_vec()),
        ]
    };
    let Value::Signed(old) = first.reader().get(block, 0, &path(), true).unwrap() else {
        panic!("expected frame int")
    };
    let foreign = TypedWrite {
        field: second.field(block, 0, path()).unwrap(),
        sdna_type: b"int".to_vec(),
        width: 4,
        expected: TypedValue::Signed(old),
        replacement: TypedValue::Signed(old),
    };
    match plan_fields(&first, &[foreign], &[], EditMode::FormatOnly) {
        Err(e) => assert_eq!(e.kind, "snapshot_mismatch"),
        Ok(_) => panic!("foreign field accepted"),
    }
    let write = TypedWrite {
        field: first.field(block, 0, path()).unwrap(),
        sdna_type: b"int".to_vec(),
        width: 4,
        expected: TypedValue::Signed(old),
        replacement: TypedValue::Signed(old),
    };
    let plan = plan_fields(&first, &[write], &[], EditMode::FormatOnly).unwrap();
    assert_eq!(
        write_plan(&second, &plan, output, &EditOptions::default())
            .unwrap_err()
            .kind,
        "snapshot_mismatch"
    );
    assert!(!output.exists());
    let result = write_plan(&first, &plan, output, &EditOptions::default()).unwrap();
    assert!(result.complete);
    assert_eq!(
        std::fs::read(input).unwrap(),
        std::fs::read(output).unwrap()
    );
    assert_eq!(
        write_plan(&first, &plan, output, &EditOptions::default())
            .unwrap_err()
            .kind,
        "output_exists"
    );
    println!("snapshot bound fields/plan; no-op bytes exact; no-replace gate PASS");
}
