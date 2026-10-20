//! Pinned apps and launch history, keyed by launch ID and saved to `usage.tsv` next to
//! `config.toml`. The file format is described in `docs/adr/0003-usage-file-format.md`.
//!
//! `Usage` is platform independent and touches no disk. `load` and `Writer` do the I/O and must
//! stay off the UI thread.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::config;

const FILE_NAME: &str = "usage.tsv";
const FORMAT_VERSION: &str = "1";
const SECONDS_PER_DAY: u64 = 86_400;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct LaunchStats {
    count: u32,
    /// Unix time in seconds.
    last_launch: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    /// Launch IDs in the order they were pinned.
    pinned: Vec<String>,
    launches: HashMap<String, LaunchStats>,
}

impl Usage {
    pub fn pinned(&self) -> &[String] {
        &self.pinned
    }

    pub fn is_pinned(&self, launch_id: &str) -> bool {
        self.pinned.iter().any(|pinned| pinned == launch_id)
    }

    /// Pins `launch_id` after the existing pins, or unpins it. Returns whether it is now pinned.
    pub fn toggle_pin(&mut self, launch_id: &str) -> bool {
        if let Some(index) = self.pinned.iter().position(|pinned| pinned == launch_id) {
            self.pinned.remove(index);
            false
        } else {
            self.pinned.push(launch_id.to_owned());
            true
        }
    }

    pub fn record_launch(&mut self, launch_id: &str, now: u64) {
        let stats: &mut LaunchStats = self.launches.entry(launch_id.to_owned()).or_default();
        stats.count = stats.count.saturating_add(1);
        stats.last_launch = now;
    }

    /// Ranks frequently and recently launched apps higher. Zero for apps never launched.
    pub fn frecency(&self, launch_id: &str, now: u64) -> u64 {
        self.launches
            .get(launch_id)
            .map_or(0, |stats| u64::from(stats.count) * recency_weight(now.saturating_sub(stats.last_launch)))
    }

    /// Launch IDs of every launched app, most recent first.
    pub fn recent_first(&self) -> Vec<&str> {
        let mut launched: Vec<(&str, u64)> =
            self.launches.iter().map(|(launch_id, stats)| (launch_id.as_str(), stats.last_launch)).collect();
        // The launch ID breaks ties so the order never depends on hash map iteration.
        launched.sort_unstable_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(right.0)));
        launched.into_iter().map(|(launch_id, _)| launch_id).collect()
    }

    /// Parses `usage.tsv`. Malformed lines are skipped, but a missing or unknown version fails so a
    /// newer file is never overwritten with less data.
    pub fn parse(contents: &str) -> Result<Self> {
        let mut usage = Self::default();
        let mut has_version = false;

        for line in contents.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let fields: Vec<&str> = line.split('\t').collect();
            match fields.as_slice() {
                ["version", version] => {
                    if *version != FORMAT_VERSION {
                        bail!("unsupported usage file version {version}");
                    }
                    has_version = true;
                }
                ["pin", launch_id] if !launch_id.is_empty() && !usage.is_pinned(launch_id) => {
                    usage.pinned.push((*launch_id).to_owned());
                }
                ["launch", count, last_launch, launch_id] if !launch_id.is_empty() => {
                    if let (Ok(count), Ok(last_launch)) = (count.parse::<u32>(), last_launch.parse::<u64>()) {
                        usage.launches.insert((*launch_id).to_owned(), LaunchStats { count, last_launch });
                    }
                }
                _ => {}
            }
        }

        if !has_version && contents.lines().any(|line| !line.is_empty() && !line.starts_with('#')) {
            bail!("the usage file has no version line");
        }

        Ok(usage)
    }

    pub fn serialize(&self) -> String {
        let mut contents = format!(
            "# Rusty Start pinned apps and launch history. Fields are tab-separated.\nversion\t{FORMAT_VERSION}\n"
        );

        for launch_id in self.pinned.iter().filter(|launch_id| is_storable(launch_id)) {
            let _ = writeln!(contents, "pin\t{launch_id}");
        }

        let mut launches: Vec<(&String, &LaunchStats)> =
            self.launches.iter().filter(|(launch_id, _)| is_storable(launch_id)).collect();
        launches.sort_unstable_by_key(|(launch_id, _)| *launch_id);
        for (launch_id, stats) in launches {
            let _ = writeln!(contents, "launch\t{}\t{}\t{launch_id}", stats.count, stats.last_launch);
        }

        contents
    }
}

/// Day-bucketed weight, so a burst of old launches fades behind apps used this week.
fn recency_weight(age_seconds: u64) -> u64 {
    match age_seconds / SECONDS_PER_DAY {
        0 => 100,
        1..=6 => 70,
        7..=29 => 50,
        30..=89 => 30,
        _ => 10,
    }
}

/// Fields are tab-separated and records newline-terminated. File paths and AppUserModelIDs never
/// contain those characters, so an ID that does is not a real app and is not saved.
fn is_storable(launch_id: &str) -> bool {
    !launch_id.is_empty() && !launch_id.contains(['\t', '\n', '\r'])
}

pub fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs())
}

fn file_path() -> Result<PathBuf> {
    config::data_dir().map(|dir| dir.join(FILE_NAME)).context("failed to determine the configuration directory")
}

/// Reads `usage.tsv`. A missing file is an empty history.
pub fn load() -> Result<Usage> {
    let path: PathBuf = file_path()?;

    match fs::read_to_string(&path) {
        Ok(contents) => Usage::parse(&contents).context("failed to parse usage.tsv"),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Usage::default()),
        Err(error) => Err(error).context("failed to read usage.tsv"),
    }
}

enum Message {
    Save(String),
    Stop,
}

/// Saves usage snapshots on a background thread, so launches and pins never wait on the disk.
/// Dropping it writes any queued snapshot and then stops the thread.
pub struct Writer {
    sender: Sender<Message>,
    thread: Option<JoinHandle<()>>,
}

impl Writer {
    pub fn spawn() -> Result<Self> {
        let path: PathBuf = file_path()?;
        let (sender, receiver) = mpsc::channel::<Message>();

        let thread: JoinHandle<()> = thread::Builder::new()
            .name("rusty-start-usage".into())
            .spawn(move || run_writer(&path, &receiver))
            .context("failed to spawn the usage writer thread")?;

        Ok(Self { sender, thread: Some(thread) })
    }

    pub fn save(&self, usage: &Usage) {
        if self.sender.send(Message::Save(usage.serialize())).is_err() {
            eprintln!("failed to queue a usage save: the writer thread has exited");
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        let _ = self.sender.send(Message::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run_writer(path: &Path, receiver: &Receiver<Message>) {
    while let Ok(Message::Save(mut contents)) = receiver.recv() {
        // Only the newest snapshot matters when saves queue up behind a slow disk.
        let mut stop = false;
        while let Ok(message) = receiver.try_recv() {
            match message {
                Message::Save(newer) => contents = newer,
                Message::Stop => {
                    stop = true;
                    break;
                }
            }
        }

        if let Err(error) = write_replacing(path, &contents) {
            eprintln!("failed to save usage.tsv: {error:#}");
        }
        if stop {
            break;
        }
    }
}

/// Writes a temporary file and renames it over `path`, so a crash never leaves a truncated file.
fn write_replacing(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("failed to create the configuration directory")?;
    }

    let temporary: PathBuf = path.with_extension("tsv.tmp");
    fs::write(&temporary, contents).context("failed to write the temporary usage file")?;
    fs::rename(&temporary, path).context("failed to replace usage.tsv")
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    #[test]
    fn toggling_a_pin_twice_unpins_it() {
        let mut usage = Usage::default();

        assert!(usage.toggle_pin("notepad"));
        assert!(usage.is_pinned("notepad"));
        assert!(!usage.toggle_pin("notepad"));
        assert!(!usage.is_pinned("notepad"));
    }

    #[test]
    fn pins_keep_the_order_they_were_added_in() {
        let mut usage = Usage::default();
        usage.toggle_pin("terminal");
        usage.toggle_pin("edge");
        usage.toggle_pin("paint");
        usage.toggle_pin("edge");

        assert_eq!(usage.pinned(), ["terminal", "paint"]);
    }

    #[test]
    fn never_launched_apps_have_zero_frecency() {
        assert_eq!(Usage::default().frecency("notepad", NOW), 0);
    }

    #[test]
    fn frecency_grows_with_each_launch() {
        let mut usage = Usage::default();
        usage.record_launch("notepad", NOW);
        let once: u64 = usage.frecency("notepad", NOW);
        usage.record_launch("notepad", NOW);

        assert!(usage.frecency("notepad", NOW) > once);
    }

    #[test]
    fn recent_launches_outrank_more_frequent_old_ones() {
        let mut usage = Usage::default();
        for _ in 0..2 {
            usage.record_launch("old", NOW - 120 * SECONDS_PER_DAY);
        }
        usage.record_launch("new", NOW);

        assert!(usage.frecency("new", NOW) > usage.frecency("old", NOW));
    }

    #[test]
    fn recent_first_orders_by_last_launch() {
        let mut usage = Usage::default();
        usage.record_launch("paint", NOW - 30);
        usage.record_launch("edge", NOW - 10);
        usage.record_launch("notepad", NOW - 20);

        assert_eq!(usage.recent_first(), ["edge", "notepad", "paint"]);
    }

    #[test]
    fn round_trips_through_the_file_format() {
        let mut usage = Usage::default();
        usage.toggle_pin(r"{6D809377-6AF0-444B-8957-A3773F02200E}\Tools\app.exe");
        usage.toggle_pin("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App");
        usage.record_launch("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App", NOW);
        usage.record_launch("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App", NOW);

        assert_eq!(Usage::parse(&usage.serialize()).unwrap(), usage);
    }

    #[test]
    fn skips_malformed_lines() {
        let contents = "version\t1\npin\t\nlaunch\tmany\t5\tpaint\nlaunch\t2\t5\tedge\nmystery\tline\n";
        let usage: Usage = Usage::parse(contents).unwrap();

        assert_eq!(usage.pinned(), Vec::<String>::new());
        assert_eq!(usage.recent_first(), ["edge"]);
    }

    #[test]
    fn rejects_unknown_versions_and_missing_version_lines() {
        assert!(Usage::parse("version\t2\npin\tpaint\n").is_err());
        assert!(Usage::parse("pin\tpaint\n").is_err());
    }

    #[test]
    fn empty_file_is_an_empty_history() {
        assert_eq!(Usage::parse("").unwrap(), Usage::default());
    }

    #[test]
    fn does_not_save_ids_containing_separators() {
        let mut usage = Usage::default();
        usage.toggle_pin("bad\tid");
        usage.record_launch("bad\nid", NOW);

        assert_eq!(Usage::parse(&usage.serialize()).unwrap(), Usage::default());
    }
}
