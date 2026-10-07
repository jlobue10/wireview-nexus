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
        ["--theme", "plaid"],
        ["--source", "usb"],
        ["--brightness", "101"],
        ["--brightness", "-1"],
        ["--fps", "0.1"],
        ["--fps", "61"],
        ["--fps", "1e10"],
        ["--fps", "1e300"],
        ["--csv-interval", "5"],
    ] {
        let (code, _, err) = run(&bad);
        assert_eq!(code, Some(2), "{bad:?}: {err}");
    }
    for bad in ["0", "0.5", "forever", "90000"] {
        let (code, _, err) = run(&["--csv-log", "x", "--csv-interval", bad]);
        assert_eq!(code, Some(2), "{bad}: {err}");
    }
}

#[test]
fn csv_log_is_written_even_without_a_nexus() {
    use std::io::Read;
    use std::process::Stdio;
    let dir = std::env::temp_dir().join(format!("wireview-nexus-csv-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let logs = dir.join("logs");
    let mut child = std::process::Command::new(BIN)
        .args(["--source", "hwinfo", "--csv-log", logs.to_str().unwrap(), "--csv-interval", "1"])
        .env("WIREVIEW_BRIDGE_SECRET", dir.join("no-such-secret"))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let file = loop {
        let found = std::fs::read_dir(&logs).ok().and_then(|d| d.flatten().next()).map(|e| e.path());
        if let Some(f) = found.filter(|f| std::fs::read_to_string(f).is_ok_and(|t| t.matches("\r\n").count() >= 3)) {
            break f;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no CSV with two rows appeared in {}",
            logs.display()
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    child.kill().unwrap();
    let mut out = String::new();
    child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
    child.wait().unwrap();
    let name = file.file_name().unwrap().to_str().unwrap().to_string();
    assert!(name.starts_with("log-") && name.ends_with(".csv") && name.len() == 23, "{name}");
    let text = std::fs::read_to_string(&file).unwrap();
    let lines: Vec<&str> = text.split("\r\n").collect();
    assert!(lines[0].starts_with("Timestamp,Connected,HW,FW,SumPowerW,"), "{}", lines[0]);
    // No HWiNFO here: rows are not-ok, with zeros, but still complete and the daemon keeps going.
    assert!(lines[1].contains(",False,,,0.000,0.000,"), "{}", lines[1]);
    assert_eq!(lines[1].split(',').count(), 22);
    assert!(out.contains("CSV log: ") && out.contains("a row every 1 s"), "{out}");
    assert!(!out.contains("logging stopped"), "{out}");
    std::fs::remove_dir_all(dir).unwrap();
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
