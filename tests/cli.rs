//! Command-line checks for the daemon; no Nexus and no WireView needed.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_wireview-nexus");

fn run(args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(BIN)
        .args(args)
        // No bridge secret here, so `auto` never trusts whatever listens on 8765.
        .env(
            "WIREVIEW_BRIDGE_SECRET",
            std::env::temp_dir().join("wireview-nexus-test-no-such-secret"),
        )
        .output()
        .unwrap();
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn d1_limits_must_be_positive_and_finite() {
    for bad in [
        ["--wire-limit", "-1"],
        ["--total-limit", "nan"],
        ["--fps", "0"],
        ["--cable-w", "inf"],
    ] {
        let (code, _, err) = run(&[bad[0], bad[1], "--preview", "unused.png", "--demo"]);
        assert_eq!(code, Some(2), "{bad:?}: {err}");
        assert!(err.contains("positive finite"), "{bad:?}: {err}");
    }
    let (code, _, err) = run(&["--wire-limit", "ten"]);
    assert!(code == Some(2) && err.contains("not a number"), "{err}");
}

#[test]
fn other_arguments_are_checked() {
    for bad in [
        ["--layout", "sideways"],
        ["--source", "usb"],
        ["--brightness", "101"],
        ["--brightness", "-1"],
        ["--fps", "0.1"],
        ["--fps", "61"],
        ["--fps", "1e10"],
        ["--fps", "1e300"],
    ] {
        let (code, _, err) = run(&bad);
        assert_eq!(code, Some(2), "{bad:?}: {err}");
    }
}

#[test]
fn d2_version() {
    let (code, out, _) = run(&["--version"]);
    assert_eq!((code, out.trim()), (Some(0), concat!("wireview-nexus ", env!("CARGO_PKG_VERSION"))));
}

fn png_size(path: &std::path::Path) -> (u32, u32) {
    let b = std::fs::read(path).unwrap();
    assert_eq!(&b[..8], b"\x89PNG\r\n\x1a\n");
    (
        u32::from_be_bytes(b[16..20].try_into().unwrap()),
        u32::from_be_bytes(b[20..24].try_into().unwrap()),
    )
}

#[test]
fn preview_writes_a_640x48_png() {
    let dir = std::env::temp_dir().join(format!("wireview-nexus-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for layout in ["combined", "per-wire", "total-current", "total-power"] {
        let png = dir.join(format!("{layout}.png"));
        let (code, out, err) = run(&["--layout", layout, "--preview", png.to_str().unwrap(), "--demo"]);
        assert_eq!(code, Some(0), "{layout}: {err}");
        assert!(out.starts_with("wrote "), "{out}");
        assert_eq!(png_size(&png), (640, 48));
    }
    // Without --demo and without a device the frame explains the problem instead.
    let png = dir.join("problem.png");
    let (code, _, err) = run(&["--source", "hwinfo", "--preview", png.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(png_size(&png), (640, 48));
    let (code, _, _) = run(&["--preview", dir.join("no/such/dir/x.png").to_str().unwrap(), "--demo"]);
    assert_eq!(code, Some(1));
    std::fs::remove_dir_all(dir).unwrap();
}
