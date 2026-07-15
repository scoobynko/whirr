# whirr v0.2 Tweaks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cap the process table at exactly 10 rows and enrich the ports card with the owning process's project folder (`:3000 node (my-app)`), per `docs/superpowers/specs/2026-07-15-whirr-v02-tweaks-design.md`.

**Architecture:** A new libproc helper reads a pid's cwd (`proc_pidinfo(PROC_PIDVNODEPATHINFO)`); the slow sampler enriches parsed ports with the cwd basename once per 10s tick. The UI caps visible processes at 10 via `App::visible_processes`, and the layout gives the freed space to the ports row.

**Tech Stack:** Existing whirr codebase (Rust, ratatui 0.29, libproc FFI conventions in `src/mac/proc.rs`).

## Global Constraints

- Data-collection code never panics; all readers return `Option`. Sudo-free. No new subprocesses.
- `cargo clippy --all-targets -- -D warnings` clean; full `cargo test` green before every commit.
- FFI struct layouts verified against the SDK header before use (record the excerpt in the report).
- Snapshot contract: `PortInfo` gains `project: Option<String>`; no other type changes.
- Process display cap: exactly **10** rows, selection clamps to index 9; sampler payload unchanged.
- Run `. "$HOME/.cargo/env"` before cargo commands. Work on branch `feat/v02-tweaks` off `main`.

---

### Task 1: `mac::proc::cwd_basename`

**Files:**
- Modify: `src/mac/proc.rs` (append; existing externs already declare `proc_pidinfo`)

**Interfaces:**
- Consumes: existing `proc_pidinfo` extern in `src/mac/proc.rs:56-62`.
- Produces: `pub fn cwd_basename(pid: i32) -> Option<String>` — basename of the process's cwd; `None` for other users' pids, dead pids, root cwd, or any FFI failure.

- [ ] **Step 1: Verify struct layout against the SDK header** (mandatory, record excerpt in report)

```bash
grep -B3 -A8 "struct vnode_info_path\|struct proc_vnodepathinfo" \
  "$(xcrun --show-sdk-path)/usr/include/sys/proc_info.h"
grep -B2 -A30 "struct vnode_info {" \
  "$(xcrun --show-sdk-path)/usr/include/sys/proc_info.h"
```

Expected: `vnode_info` = `struct vinfo_stat` + `int vi_type` + `int vi_pad` + `fsid_t vi_fsid` (152 bytes total); `vnode_info_path` = `vnode_info` + `char vip_path[MAXPATHLEN]` (1176 bytes); `proc_vnodepathinfo` = `pvi_cdir` + `pvi_rdir` (2352 bytes). If the header disagrees with these sizes, the header wins — adjust the padding constant below and note it.

- [ ] **Step 2: Write the failing test** (append to the `tests` module in `src/mac/proc.rs`)

```rust
#[test]
fn cwd_basename_of_self_matches_current_dir() {
    let expect = std::env::current_dir()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let got = super::cwd_basename(std::process::id() as i32).expect("own cwd readable");
    assert_eq!(got, expect);
}

#[test]
fn cwd_basename_of_dead_pid_is_none() {
    assert_eq!(super::cwd_basename(-1), None);
}
```

- [ ] **Step 3: Run to verify failure** — `cargo test cwd_basename` → FAIL (function undefined)

- [ ] **Step 4: Implement** (append to `src/mac/proc.rs`, below the `ProcScanner` impl)

```rust
const PROC_PIDVNODEPATHINFO: libc::c_int = 9;
const MAXPATHLEN: usize = 1024;
/// sizeof(struct vnode_info): struct vinfo_stat (136) + vi_type + vi_pad +
/// vi_fsid[2] — verified against <sys/proc_info.h>, see Task 1 Step 1.
const VNODE_INFO_SIZE: usize = 152;

/// `struct vnode_info_path` from `<sys/proc_info.h>`. The leading
/// `vnode_info` is opaque padding here — only the path matters.
#[repr(C)]
struct VnodeInfoPath {
    _vi: [u8; VNODE_INFO_SIZE],
    vip_path: [u8; MAXPATHLEN],
}

/// `struct proc_vnodepathinfo` from `<sys/proc_info.h>`.
#[repr(C)]
struct ProcVnodePathInfo {
    pvi_cdir: VnodeInfoPath,
    pvi_rdir: VnodeInfoPath,
}

/// Basename of `pid`'s current working directory. `None` for pids we may
/// not inspect (other users), dead pids, `/`, or any FFI failure.
pub fn cwd_basename(pid: i32) -> Option<String> {
    let mut info: ProcVnodePathInfo = unsafe { std::mem::zeroed() };
    let sz = std::mem::size_of::<ProcVnodePathInfo>() as libc::c_int;
    let r = unsafe {
        proc_pidinfo(
            pid,
            PROC_PIDVNODEPATHINFO,
            0,
            &mut info as *mut _ as *mut libc::c_void,
            sz,
        )
    };
    if r != sz {
        return None;
    }
    let path = &info.pvi_cdir.vip_path;
    let end = path.iter().position(|&b| b == 0).unwrap_or(path.len());
    let s = std::str::from_utf8(&path[..end]).ok()?;
    if s.is_empty() || s == "/" {
        return None;
    }
    std::path::Path::new(s)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
}
```

- [ ] **Step 5: Run to verify pass** — `cargo test cwd_basename` → 2 passed. Also full `cargo test` + `cargo clippy --all-targets -- -D warnings`.

- [ ] **Step 6: Commit**

```bash
git add src/mac/proc.rs
git commit -m "feat: cwd_basename via PROC_PIDVNODEPATHINFO"
```

---

### Task 2: Port project enrichment (sampler + UI)

**Files:**
- Modify: `src/sampler/mod.rs` (PortInfo), `src/sampler/slow.rs` (parse + enrich), `src/ui/ports.rs` (badge), `src/app.rs` (demo data)

**Interfaces:**
- Consumes: `mac::proc::cwd_basename(pid: i32) -> Option<String>` from Task 1.
- Produces: `PortInfo { port: u16, process: String, pid: i32, project: Option<String> }` — later render tests rely on the `(project)` badge text.

- [ ] **Step 1: Extend the parser tests** (in `src/sampler/slow.rs` tests module — parser always yields `project: None`; enrichment is separate)

Replace the `parses_and_dedups_ports` view tuple with a 4-field check:

```rust
#[test]
fn parses_and_dedups_ports() {
    let ports = parse_lsof(FIXTURE);
    let view: Vec<(u16, &str, i32, Option<&str>)> = ports
        .iter()
        .map(|p| (p.port, p.process.as_str(), p.pid, p.project.as_deref()))
        .collect();
    assert_eq!(
        view,
        vec![
            (3000, "node", 9001, None),
            (5432, "postgres", 512, None),
            (7000, "Control Center", 9002, None)
        ]
    );
}
```

Add an enrichment test (pure merge logic, no FFI — takes a lookup closure):

```rust
#[test]
fn enrich_fills_project_per_pid_once() {
    let mut ports = parse_lsof(FIXTURE);
    let mut calls = 0;
    enrich_projects(&mut ports, |pid| {
        calls += 1;
        (pid == 9001).then(|| "my-app".to_string())
    });
    assert_eq!(calls, 3); // one lookup per unique pid
    assert_eq!(
        ports.iter().find(|p| p.port == 3000).unwrap().project.as_deref(),
        Some("my-app")
    );
    assert_eq!(ports.iter().find(|p| p.port == 5432).unwrap().project, None);
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test slow` → FAIL (no `project` field, no `enrich_projects`)

- [ ] **Step 3: Implement**

`src/sampler/mod.rs` — add the field:

```rust
#[derive(Clone)]
pub struct PortInfo {
    pub port: u16,
    pub process: String,
    pub pid: i32,
    /// Basename of the owning process's cwd — "which project is this?".
    pub project: Option<String>,
}
```

`src/sampler/slow.rs` — parser fills `project: None`; add enrichment and call it in `run()`:

```rust
use std::collections::HashMap;

// in parse_lsof's or_insert_with:
by_port.entry(port).or_insert_with(|| PortInfo {
    port,
    process: cmd.clone(),
    pid,
    project: None,
});

/// Fill `project` via `lookup`, calling it once per unique pid (a process
/// often listens on several ports).
pub fn enrich_projects(
    ports: &mut [PortInfo],
    mut lookup: impl FnMut(i32) -> Option<String>,
) {
    let mut cache: HashMap<i32, Option<String>> = HashMap::new();
    for p in ports.iter_mut() {
        p.project = cache.entry(p.pid).or_insert_with(|| lookup(p.pid)).clone();
    }
}

// in run(), success arm only (the parse arm):
let mut ports = parse_lsof(&String::from_utf8_lossy(&out.stdout));
enrich_projects(&mut ports, crate::mac::proc::cwd_basename);
last_good = ports;
SlowSnap { ports: last_good.clone(), stale: false }
```

`src/ui/ports.rs` — after the process-name span, add the project span:

```rust
if let Some(project) = &p.project {
    spans.push(Span::styled(
        format!("({project}) "),
        if selected { style } else { Style::default().fg(theme::DIM) },
    ));
}
```

`src/app.rs` — in `App::demo()`'s SlowSnap, give one port `project: Some("my-app".into())` and the others `None` (adjust the existing constructors for the new field).

- [ ] **Step 4: Run to verify pass** — `cargo test` full suite green (render tests still pass — demo change is additive); `cargo clippy --all-targets -- -D warnings` clean.

- [ ] **Step 5: Commit**

```bash
git add src/sampler/mod.rs src/sampler/slow.rs src/ui/ports.rs src/app.rs
git commit -m "feat: ports card shows owning project folder"
```

---

### Task 3: Top-10 process table + layout rebalance

**Files:**
- Modify: `src/app.rs` (visible_processes cap + test), `src/ui/mod.rs` (layout), `tests/render.rs` (geometry + project assertions)

**Interfaces:**
- Consumes: `App::visible_processes()` (used by `src/ui/processes.rs:26` and `focused_len` — capping it caps both display and selection).
- Produces: `visible_processes()` returns at most 10 entries; ui::draw middle row `Length(13)`, ports row `Min(4)`.

- [ ] **Step 1: Write the failing tests**

In `src/app.rs` tests module:

```rust
#[test]
fn process_view_caps_at_ten() {
    let mut a = App::new(false);
    let procs: Vec<ProcInfo> = (0..30)
        .map(|i| ProcInfo {
            pid: i,
            name: format!("p{i}"),
            cpu: 30.0 - i as f32,
            mem: 100,
        })
        .collect();
    let mut f = demo_fast(); // extract a helper from app_with_procs, or build inline
    f.processes = procs;
    a.ingest(Snapshot::Fast(f));
    assert_eq!(a.visible_processes().len(), 10);
    for _ in 0..15 {
        a.on_key(KeyEvent::from(KeyCode::Down));
    }
    assert_eq!(a.selected, 9);
}
```

(If no `demo_fast` helper exists, build the `FastSnap` inline with the same fields the existing `app_with_procs` test uses.)

In `tests/render.rs`, extend `full_size_shows_all_panels`:

```rust
assert!(c.contains("(my-app)"), "port project badge missing");
```

- [ ] **Step 2: Run to verify failure** — `cargo test process_view_caps` → FAIL (returns 30); render test FAIL only if demo's project port isn't rendering (should pass from Task 2 — if it already passes, note it as verified-by-existing).

- [ ] **Step 3: Implement**

`src/app.rs`:

```rust
/// The process table shows at most this many rows; selection clamps with it.
pub const MAX_VISIBLE_PROCS: usize = 10;

pub fn visible_processes(&self) -> &[ProcInfo] {
    self.fast.as_ref().map_or(&[], |f| {
        &f.processes[..f.processes.len().min(MAX_VISIBLE_PROCS)]
    })
}
```

(`focused_len` already routes through `f.processes.len()` for Processes — change it to `self.visible_processes().len()` so selection clamps to the cap:)

```rust
fn focused_len(&self) -> usize {
    match self.focus {
        Focus::Processes => self.visible_processes().len(),
        Focus::Ports => self.slow.as_ref().map_or(0, |s| s.ports.len()),
    }
}
```

`src/ui/mod.rs` — middle row fixed, ports absorb the rest:

```rust
let mut rows = vec![Constraint::Length(3), Constraint::Length(10)];
if show_ports {
    // 10 process rows + footer + borders; ports card takes what's left
    rows.push(Constraint::Length(13));
    rows.push(Constraint::Min(4));
} else {
    rows.push(Constraint::Min(6));
}
let chunks = Layout::vertical(rows).split(area);
```

(The rest of `draw` is unchanged — `chunks[2]` is still the middle row and `chunks[3]` the ports row, same indices as today.)

- [ ] **Step 4: Run to verify pass** — full `cargo test` green (render size-sweep must still pass — the collapse thresholds are unchanged); `cargo clippy --all-targets -- -D warnings` clean. `cargo build --release` (refreshes the installed symlink target).

- [ ] **Step 5: Commit**

```bash
git add src/app.rs src/ui/mod.rs tests/render.rs
git commit -m "feat: cap process table at top 10, grow ports card"
```
