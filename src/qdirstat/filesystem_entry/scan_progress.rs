use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::io::Write;

use console::Term;

use super::filesystem_entry_extensions::BytesExt;
use super::volume::VolumeUsage;

const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
const FILLED: char = '█';
const EMPTY: char = '░';
const MINIMUM_PATH_COLUMNS: usize = 8;
/// Below this coverage the scan is visibly missing data and says so.
const COVERAGE_WORTH_REPORTING: u8 = 90;
const REDRAW_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

/// Counters shared between the scanning thread and the thread drawing the status line.
/// The scan only ever writes and the renderer only ever reads, so relaxed ordering is enough:
/// a counter that lands a redraw late simply shows up on the next frame.
pub struct ScanProgress {
    entries: AtomicU64,
    bytes: AtomicU64,
    skipped: AtomicU64,
    finished: AtomicBool,
    current: Mutex<String>,
    /// What the filesystem reports about the volume, when the platform could say.
    /// Fixed for the run, so it needs no synchronisation.
    volume: Option<VolumeUsage>,
}

impl ScanProgress {
    pub fn new(volume: Option<VolumeUsage>) -> ScanProgress {
        ScanProgress {
            entries: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            skipped: AtomicU64::new(0),
            finished: AtomicBool::new(false),
            current: Mutex::new(String::new()),
            volume,
        }
    }

    pub fn record_file(&self, size: u64) {
        self.entries.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(size, Ordering::Relaxed);
    }

    /// Directories are counted but not sized: their size is the sum of their children,
    /// which are recorded individually.
    pub fn record_directory(&self) {
        self.entries.fetch_add(1, Ordering::Relaxed);
    }

    /// An entry whose metadata could not be read, so its size is unknown.
    pub fn record_skipped(&self) {
        self.skipped.fetch_add(1, Ordering::Relaxed);
    }

    pub fn enter_directory(&self, path: &str) {
        // A poisoned lock only means the renderer panicked mid-read; the path shown is
        // cosmetic, so recovering the value beats bringing the scan down with it.
        let mut current = self.current.lock().unwrap_or_else(|e| e.into_inner());
        current.clear();
        current.push_str(path);
    }

    pub fn finish(&self) {
        self.finished.store(true, Ordering::Release);
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    pub fn entries(&self) -> u64 {
        self.entries.load(Ordering::Relaxed)
    }

    pub fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }

    pub fn skipped(&self) -> u64 {
        self.skipped.load(Ordering::Relaxed)
    }

    fn current(&self) -> String {
        self.current.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// How much of the volume's used space the scan has walked so far -- the figure the
    /// progress bar advances on. This is scan *coverage*, not how full the disk is; by
    /// the end of a healthy scan it sits just under 100% and says nothing about capacity.
    ///
    /// It is deliberately approximate. The numerator is the sum of logical file sizes,
    /// which does not equal bytes on disk: hardlinks are counted once per link, sparse
    /// and compressed files report more than they occupy, and the scan skips dotfiles
    /// and `$`-prefixed entries entirely. Hence the clamp.
    pub fn percent(&self) -> Option<u8> {
        let volume = self.volume.as_ref()?;
        if volume.used == 0 {
            return None;
        }

        // u128 so a large volume cannot overflow the multiply.
        let scaled = self.bytes() as u128 * 100 / volume.used as u128;

        // Truncating rather than rounding: 100% should mean finished, not nearly.
        Some(scaled.min(100) as u8)
    }

    /// One line of status, sized to fit `width` columns so it never wraps.
    fn status_line(&self, spinner: char, width: usize) -> String {
        let progress = match self.percent() {
            None => String::new(),
            Some(percent) => {
                // The bar is the first thing to go when columns get tight; the number
                // carries the same information in a fraction of the space.
                let cells = if width >= 100 {
                    20
                } else if width >= 76 {
                    12
                } else {
                    0
                };

                if cells == 0 {
                    format!("{:>3}%  ", percent)
                } else {
                    format!("{} {:>3}%  ", bar(percent, cells), percent)
                }
            }
        };

        let head = format!(
            "  {}  {}{} entries   {}   ",
            spinner,
            progress,
            with_thousands(self.entries()),
            self.bytes().bytes_to_readable()
        );

        // Leave a column spare: filling the last one wraps on some terminals.
        let budget = width.saturating_sub(head.chars().count() + 1);

        // Under a handful of columns a truncated path is all ellipsis and no path, which
        // reads as a stray mark rather than information. Better to show nothing.
        let path = if budget >= MINIMUM_PATH_COLUMNS {
            truncate(self.current().as_str(), budget)
        } else {
            String::new()
        };

        let line = format!("{}{}", head, path);

        // A narrow terminal can be too small for even the counters, which would leave the
        // head itself overflowing. The line has to fit whatever room there actually is.
        truncate(line.trim_end(), width.saturating_sub(1))
    }

    /// The one-line note about unreadable entries, or None when everything could be read.
    pub fn skipped_warning(&self) -> Option<String> {
        let skipped = self.skipped();

        if skipped == 0 {
            None
        } else if skipped == 1 {
            Some(String::from(
                "1 entry could not be read and is missing from the totals. Consider running as admin.",
            ))
        } else {
            Some(format!(
                "{} entries could not be read and are missing from the totals. Consider running as admin.",
                with_thousands(skipped)
            ))
        }
    }

    pub fn summary(&self, elapsed: std::time::Duration) -> String {
        format!(
            "Scanned {} entries ({}) in {}.",
            with_thousands(self.entries()),
            self.bytes().bytes_to_readable(),
            format_duration(elapsed)
        )
    }

    /// How full the disk is, straight from the filesystem. This is the number worth
    /// reading after a scan, and it is not derived from anything the scan counted.
    pub fn volume_line(&self) -> Option<String> {
        let volume = self.volume.as_ref()?;

        Some(format!(
            "Volume is {}% full: {} used of {}.",
            volume.percent_full(),
            volume.used.bytes_to_readable(),
            volume.total.bytes_to_readable()
        ))
    }

    /// Coverage is only worth a line when it is poor enough to mean the totals below are
    /// missing something real. A healthy scan sits in the high nineties, where saying so
    /// every time is noise that reads like a fullness figure.
    pub fn coverage_warning(&self) -> Option<String> {
        let volume = self.volume.as_ref()?;
        let coverage = self.percent()?;

        if coverage >= COVERAGE_WORTH_REPORTING {
            return None;
        }

        Some(format!(
            "This scan accounted for {}% of the {} in use. The rest is filtered, unreadable, or outside the scanned tree.",
            coverage,
            volume.used.bytes_to_readable()
        ))
    }
}

/// Redraws a single status line in place until the scan reports itself finished.
/// Returns without drawing anything when output is redirected, where a rewriting
/// line would only produce carriage-return noise in the captured text.
pub fn render_loop(progress: &ScanProgress) {
    let mut term = Term::stdout();
    if !term.is_term() {
        return;
    }

    let mut frame = 0usize;
    while !progress.is_finished() {
        let width = term.size().1 as usize;
        let line = progress.status_line(FRAMES[frame % FRAMES.len()], width);

        let _ = term.clear_line();
        let _ = term.write_all(line.as_bytes());
        let _ = term.flush();

        frame += 1;
        std::thread::sleep(REDRAW_INTERVAL);
    }

    // Hand a clean line back to the caller so the summary starts from column zero.
    let _ = term.clear_line();
    let _ = term.flush();
}

fn bar(percent: u8, cells: usize) -> String {
    let filled = percent as usize * cells / 100;

    let mut rendered = String::with_capacity(cells * 3 + 2);
    rendered.push('[');
    for cell in 0..cells {
        rendered.push(if cell < filled { FILLED } else { EMPTY });
    }
    rendered.push(']');

    rendered
}

fn truncate(text: &str, budget: usize) -> String {
    if budget == 0 {
        return String::new();
    }

    // Counting characters rather than bytes: paths are not always ASCII.
    if text.chars().count() <= budget {
        return text.to_string();
    }

    if budget <= 3 {
        return ".".repeat(budget);
    }

    let kept = text.chars().take(budget - 3).collect::<String>();
    format!("{}...", kept)
}

fn with_thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);

    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }

    grouped
}

fn format_duration(elapsed: std::time::Duration) -> String {
    let seconds = elapsed.as_secs();

    if seconds >= 60 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else if seconds >= 10 {
        format!("{}s", seconds)
    } else {
        format!("{:.1}s", elapsed.as_secs_f64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A volume of `used` bytes in use. Capacity only matters to the fullness figure,
    /// so default it to double the usage: half full.
    fn volume(used: u64) -> Option<VolumeUsage> {
        Some(VolumeUsage { total: used * 2, used })
    }

    #[test]
    fn groups_digits_in_threes() {
        assert_eq!(with_thousands(0), "0");
        assert_eq!(with_thousands(999), "999");
        assert_eq!(with_thousands(1000), "1,000");
        assert_eq!(with_thousands(512880), "512,880");
        assert_eq!(with_thousands(1234567890), "1,234,567,890");
    }

    #[test]
    fn truncation_respects_the_budget() {
        assert_eq!(truncate("C:\\Users", 0), "");
        assert_eq!(truncate("C:\\Users", 32), "C:\\Users");
        assert_eq!(truncate("C:\\Users\\qlaut\\AppData", 11), "C:\\Users...");
        assert_eq!(truncate("C:\\Users", 2), "..");
    }

    #[test]
    fn truncation_never_splits_a_character() {
        // Four characters, but ten bytes of UTF-8.
        let path = "/tmp/日本語です";
        let truncated = truncate(path, 8);
        assert_eq!(truncated.chars().count(), 8);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn status_line_fits_the_terminal_width() {
        let progress = ScanProgress::new(None);
        progress.record_file(4096);
        progress.enter_directory("C:\\Users\\qlaut\\AppData\\Local\\Packages\\SomethingLong");

        for width in [20usize, 40, 80, 120] {
            let line = progress.status_line('⠋', width);
            assert!(
                line.chars().count() < width,
                "width {} produced {} chars",
                width,
                line.chars().count()
            );
        }
    }

    #[test]
    fn counters_separate_files_from_directories() {
        let progress = ScanProgress::new(None);
        progress.record_file(1000);
        progress.record_file(500);
        progress.record_directory();
        progress.record_skipped();

        assert_eq!(progress.entries(), 3, "directories count toward entries");
        assert_eq!(progress.bytes(), 1500, "directories contribute no bytes");
        assert_eq!(progress.skipped(), 1);
    }

    #[test]
    fn a_path_too_cramped_to_read_is_dropped_rather_than_stubbed() {
        let progress = ScanProgress::new(volume(1000));
        progress.record_file(500);
        progress.enter_directory("C:/Users/qlaut/AppData/Local");

        let cramped = progress.status_line('\u{283b}', 40);
        assert!(!cramped.ends_with('.'), "no dot stub left behind: {}", cramped);
        assert!(cramped.contains("50%"), "the counters still survive: {}", cramped);
    }

    #[test]
    fn status_lines_carry_no_trailing_whitespace() {
        let progress = ScanProgress::new(volume(1000));
        progress.record_file(500);
        progress.enter_directory("C:/Users");

        for width in [24usize, 40, 60, 80, 120] {
            let line = progress.status_line('\u{283b}', width);
            assert_eq!(line, line.trim_end(), "width {} left trailing space", width);
        }
    }

    #[test]
    fn percent_is_none_without_a_usable_volume_size() {
        assert_eq!(ScanProgress::new(None).percent(), None, "platform could not say");
        assert_eq!(ScanProgress::new(volume(0)).percent(), None, "no dividing by zero");
    }

    #[test]
    fn percent_tracks_bytes_against_the_volume() {
        let progress = ScanProgress::new(volume(1000));
        assert_eq!(progress.percent(), Some(0));

        progress.record_file(250);
        assert_eq!(progress.percent(), Some(25));

        progress.record_file(250);
        assert_eq!(progress.percent(), Some(50));
    }

    #[test]
    fn percent_truncates_so_it_never_reads_full_early() {
        let progress = ScanProgress::new(volume(1000));
        progress.record_file(999);
        assert_eq!(progress.percent(), Some(99), "99.9% is not 100%");
    }

    #[test]
    fn percent_clamps_when_logical_sizes_exceed_the_volume() {
        // Hardlinks and sparse files can push the sum past what the disk holds.
        let progress = ScanProgress::new(volume(1000));
        progress.record_file(4000);
        assert_eq!(progress.percent(), Some(100), "clamped, not 400");
    }

    #[test]
    fn percent_does_not_overflow_on_a_large_volume() {
        let sixteen_tb = 16u64 * 1024 * 1024 * 1024 * 1024;
        let progress = ScanProgress::new(volume(sixteen_tb));
        progress.record_file(sixteen_tb / 4);
        assert_eq!(progress.percent(), Some(25));
    }

    #[test]
    fn bar_fills_in_proportion() {
        assert_eq!(bar(0, 10), "[░░░░░░░░░░]");
        assert_eq!(bar(50, 10), "[█████░░░░░]");
        assert_eq!(bar(100, 10), "[██████████]");
        assert_eq!(bar(7, 10), "[░░░░░░░░░░]", "below one cell stays empty");
    }

    #[test]
    fn wide_terminals_get_a_bar_and_narrow_ones_only_the_number() {
        let progress = ScanProgress::new(volume(1000));
        progress.record_file(500);

        assert!(progress.status_line('⠋', 120).contains(FILLED), "120 columns fits a bar");
        assert!(progress.status_line('⠋', 80).contains(FILLED), "80 columns fits a bar");

        let narrow = progress.status_line('⠋', 60);
        assert!(!narrow.contains(FILLED), "60 columns drops the bar: {}", narrow);
        assert!(narrow.contains("50%"), "but keeps the number: {}", narrow);
    }

    #[test]
    fn summary_says_what_was_scanned_and_nothing_about_capacity() {
        let progress = ScanProgress::new(volume(1000));
        progress.record_file(400);

        let summary = progress.summary(std::time::Duration::from_secs(1));
        assert!(summary.contains("400 Bytes"), "{}", summary);
        assert!(!summary.contains('%'), "fullness belongs on its own line: {}", summary);
    }

    #[test]
    fn volume_line_reports_fullness_not_scan_coverage() {
        // 1000 of 2000 bytes in use, of which the scan walked only 400.
        let progress = ScanProgress::new(volume(1000));
        progress.record_file(400);

        let line = progress.volume_line().expect("a volume was reported");
        assert!(line.contains("50% full"), "fullness is used over capacity: {}", line);
        assert!(!line.contains("40%"), "coverage must not leak into it: {}", line);

        assert_eq!(progress.percent(), Some(40), "coverage remains its own figure");
    }

    #[test]
    fn volume_line_is_absent_when_the_platform_cannot_say() {
        assert!(ScanProgress::new(None).volume_line().is_none());
    }

    #[test]
    fn coverage_is_mentioned_only_when_it_is_poor() {
        // The regression this guards: a near-complete scan announcing a percentage every
        // time, which reads as a fullness figure and is not one.
        let healthy = ScanProgress::new(volume(1000));
        healthy.record_file(990);
        assert_eq!(healthy.percent(), Some(99));
        assert!(healthy.coverage_warning().is_none(), "99% coverage is not news");

        let poor = ScanProgress::new(volume(1000));
        poor.record_file(300);
        let warning = poor.coverage_warning().expect("30% coverage warrants a line");
        assert!(warning.contains("30%"), "{}", warning);
    }

    #[test]
    fn skipped_warning_appears_only_when_needed_and_agrees_in_number() {
        let progress = ScanProgress::new(None);
        assert!(progress.skipped_warning().is_none(), "nothing skipped, nothing to say");

        progress.record_skipped();
        let one = progress.skipped_warning().expect("one skip warns");
        assert!(one.starts_with("1 entry could not be read and is missing"), "{}", one);

        progress.record_skipped();
        let two = progress.skipped_warning().expect("two skips warn");
        assert!(two.starts_with("2 entries could not be read and are missing"), "{}", two);

        for _ in 0..1998 {
            progress.record_skipped();
        }
        assert!(progress.skipped_warning().unwrap().starts_with("2,000 entries"));
    }

    #[test]
    fn durations_gain_precision_when_short() {
        use std::time::Duration;
        assert_eq!(format_duration(Duration::from_millis(1500)), "1.5s");
        assert_eq!(format_duration(Duration::from_secs(42)), "42s");
        assert_eq!(format_duration(Duration::from_secs(125)), "2m 5s");
    }
}
