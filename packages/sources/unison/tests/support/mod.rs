use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
pub fn ucm() -> Option<std::ffi::OsString> {
    let bin = std::env::var_os("MOONKALE_UCM").unwrap_or_else(|| "ucm".into());
    let mut child = match Command::new(&bin)
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("SKIP Unison integration fixture: UCM missing; set MOONKALE_UCM or PATH");
            return None;
        }
        Err(e) => panic!("cannot start UCM version probe: {e}"),
    };
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "UCM version probe failed");
            return Some(bin);
        }
        if start.elapsed() > Duration::from_secs(5) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("UCM version probe timed out");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub fn transcript(bin: &std::ffi::OsStr, dir: &Path, codebase: &Path, text: &str) {
    let file = dir.join("fixture.md");
    std::fs::write(&file, text).unwrap();
    let log = dir.join("transcript.log");
    let output = std::fs::File::create(&log).unwrap();
    let mut child = Command::new(bin)
        .arg("-C")
        .arg(codebase)
        .arg("transcript.in-place")
        .arg(&file)
        .env("XDG_DATA_HOME", dir.join("data"))
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(output.try_clone().unwrap())
        .stderr(output)
        .spawn()
        .unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if start.elapsed() > Duration::from_secs(30)
            || std::fs::metadata(&log).unwrap().len() > 2 * 1024 * 1024
        {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("fixture UCM timeout/output cap");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let log = std::fs::read_to_string(log).unwrap();
    assert!(
        status.success() && !log.contains("transcript failed") && !log.contains("error occurred"),
        "{log}"
    );
}
pub const INITIAL: &str = r#"
```ucm
> project.create-empty fixture
fixture/main> builtins.merge
fixture/main> alias.type builtin.Boolean Boolean
fixture/main> delete.namespace.force builtin
```
```unison
structural type Choice = Left | Right
structural ability Ask where
  ask : Choice
test> ns.check = []
ns.a x = true
ns.b x = if ns.a x then false else true
ns.c x = ns.b x
ns.even x = if x then ns.odd false else true
ns.odd x = if x then ns.even false else false
ns.pick = Choice.Left
```
```ucm
fixture/main> add
```
"#;
