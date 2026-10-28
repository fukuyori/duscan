use std::{
    cmp::Ordering,
    env, fs, io,
    os::raw::c_int,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::raw::{c_char, c_long};

#[cfg(unix)]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct Tm {
    tm_sec: c_int,
    tm_min: c_int,
    tm_hour: c_int,
    tm_mday: c_int,
    tm_mon: c_int,
    tm_year: c_int,
    tm_wday: c_int,
    tm_yday: c_int,
    tm_isdst: c_int,
    tm_gmtoff: c_long,
    tm_zone: *const c_char,
}

#[cfg(windows)]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct Tm {
    tm_sec: c_int,
    tm_min: c_int,
    tm_hour: c_int,
    tm_mday: c_int,
    tm_mon: c_int,
    tm_year: c_int,
    tm_wday: c_int,
    tm_yday: c_int,
    tm_isdst: c_int,
}

#[cfg(unix)]
unsafe extern "C" {
    fn localtime_r(timep: *const c_long, result: *mut Tm) -> *mut Tm;
}

#[cfg(windows)]
unsafe extern "C" {
    fn _localtime64_s(result: *mut Tm, timep: *const i64) -> c_int;
}

#[derive(Debug, Default)]
struct Usage {
    bytes: u64,
    files: u64,
    oldest: Option<SystemTime>,
    newest: Option<SystemTime>,
}

impl Usage {
    fn add_file(&mut self, bytes: u64, modified: Option<SystemTime>) {
        self.bytes = self.bytes.saturating_add(bytes);
        self.files += 1;

        if let Some(modified) = modified {
            self.oldest = Some(match self.oldest {
                Some(current) => current.min(modified),
                None => modified,
            });
            self.newest = Some(match self.newest {
                Some(current) => current.max(modified),
                None => modified,
            });
        }
    }
}

#[derive(Debug)]
struct Row {
    name: String,
    usage: Usage,
}

#[derive(Debug)]
struct Config {
    target: PathBuf,
    depth: usize,
    sort: SortKey,
    order: SortOrder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortKey {
    None,
    Size,
    Files,
    Newest,
    Oldest,
}

#[derive(Clone, Copy, Debug)]
enum SortOrder {
    Asc,
    Desc,
}

fn main() {
    let config = match parse_args() {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            eprintln!();
            print_usage();
            std::process::exit(2);
        }
    };

    if let Err(error) = run(&config) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn parse_args() -> Result<Config, String> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let mut index = 0;
    let mut target = None;
    let mut depth = 1;
    let mut sort = SortKey::None;
    let mut order = SortOrder::Desc;

    while index < args.len() {
        let arg = &args[index];
        let arg_string = arg.to_string_lossy();

        match arg_string.as_ref() {
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            "-v" | "--version" => {
                println!("duscan {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "-d" | "--depth" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("error: --depth requires a value".to_string());
                };
                depth = parse_depth(&value.to_string_lossy())?;
            }
            value if value.starts_with("--depth=") => {
                depth = parse_depth(value.trim_start_matches("--depth="))?;
            }
            "-s" | "--sort" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("error: --sort requires a value".to_string());
                };
                sort = parse_sort_key(&value.to_string_lossy())?;
                order = default_order(sort);

                if let Some(value) = args.get(index + 1) {
                    let value = value.to_string_lossy();
                    if let Some(parsed_order) = parse_sort_order_optional(&value) {
                        order = parsed_order;
                        index += 1;
                    } else if !value.starts_with('-') && args.get(index + 2).is_some() {
                        return Err(format!(
                            "error: invalid sort order: {value} (expected asc or desc)"
                        ));
                    }
                }
            }
            value if value.starts_with("--sort=") => {
                sort = parse_sort_key(value.trim_start_matches("--sort="))?;
                order = default_order(sort);

                if let Some(value) = args.get(index + 1) {
                    let value = value.to_string_lossy();
                    if let Some(parsed_order) = parse_sort_order_optional(&value) {
                        order = parsed_order;
                        index += 1;
                    } else if !value.starts_with('-') && args.get(index + 2).is_some() {
                        return Err(format!(
                            "error: invalid sort order: {value} (expected asc or desc)"
                        ));
                    }
                }
            }
            _ if target.is_none() => {
                target = Some(PathBuf::from(arg));
            }
            _ => {
                return Err(format!("error: unexpected argument: {arg_string}"));
            }
        }

        index += 1;
    }

    Ok(Config {
        target: target.unwrap_or_else(|| PathBuf::from(".")),
        depth,
        sort,
        order,
    })
}

fn parse_depth(value: &str) -> Result<usize, String> {
    let depth = value
        .parse::<usize>()
        .map_err(|_| format!("error: invalid depth: {value}"))?;

    if depth == 0 {
        return Err("error: depth must be 1 or greater".to_string());
    }

    Ok(depth)
}

fn parse_sort_key(value: &str) -> Result<SortKey, String> {
    match value {
        "none" => Ok(SortKey::None),
        "size" => Ok(SortKey::Size),
        "files" => Ok(SortKey::Files),
        "newest" => Ok(SortKey::Newest),
        "oldest" => Ok(SortKey::Oldest),
        _ => Err(format!(
            "error: invalid sort key: {value} (expected none, size, files, newest, or oldest)"
        )),
    }
}

fn parse_sort_order_optional(value: &str) -> Option<SortOrder> {
    match value {
        "asc" => Some(SortOrder::Asc),
        "desc" => Some(SortOrder::Desc),
        _ => None,
    }
}

fn default_order(sort: SortKey) -> SortOrder {
    match sort {
        SortKey::Oldest => SortOrder::Asc,
        SortKey::None | SortKey::Size | SortKey::Files | SortKey::Newest => SortOrder::Desc,
    }
}

fn print_usage() {
    eprintln!(
        "usage: duscan [--depth N] [--sort none|size|files|newest|oldest [asc|desc]] [-v] [PATH]"
    );
    eprintln!("       (default: none, results are streamed out as they are found)");
}

fn run(config: &Config) -> io::Result<()> {
    if config.sort == SortKey::None {
        print_header();
        let mut sink = |row: Row| print_row(&row);
        collect_rows(&config.target, &config.target, 1, config.depth, &mut sink)?;
        return Ok(());
    }

    let mut rows = Vec::new();
    let mut sink = |row: Row| rows.push(row);
    collect_rows(&config.target, &config.target, 1, config.depth, &mut sink)?;

    sort_rows(&mut rows, config.sort, config.order);

    print_header();
    for row in &rows {
        print_row(row);
    }
    Ok(())
}

fn sort_rows(rows: &mut [Row], sort: SortKey, order: SortOrder) {
    rows.sort_by(|left, right| {
        compare_rows(left, right, sort, order).then_with(|| left.name.cmp(&right.name))
    });
}

fn compare_rows(left: &Row, right: &Row, sort: SortKey, order: SortOrder) -> Ordering {
    match sort {
        SortKey::None => Ordering::Equal,
        SortKey::Size => compare_values(left.usage.bytes, right.usage.bytes, order),
        SortKey::Files => compare_values(left.usage.files, right.usage.files, order),
        SortKey::Newest => compare_times(left.usage.newest, right.usage.newest, order),
        SortKey::Oldest => compare_times(left.usage.oldest, right.usage.oldest, order),
    }
}

fn compare_values<T: Ord>(left: T, right: T, order: SortOrder) -> Ordering {
    match order {
        SortOrder::Asc => left.cmp(&right),
        SortOrder::Desc => right.cmp(&left),
    }
}

fn compare_times(
    left: Option<SystemTime>,
    right: Option<SystemTime>,
    order: SortOrder,
) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => compare_values(left, right, order),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn collect_rows<F: FnMut(Row)>(
    target: &Path,
    path: &Path,
    current_depth: usize,
    max_depth: usize,
    sink: &mut F,
) -> io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                eprintln!("warning: failed to read an entry: {error}");
                continue;
            }
        };

        let path = entry.path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                eprintln!("warning: failed to read {}: {error}", path.display());
                continue;
            }
        };

        if metadata.is_dir() && current_depth < max_depth {
            if is_empty_dir(&path) {
                sink(row_for_path(target, &path));
            } else if let Err(error) =
                collect_rows(target, &path, current_depth + 1, max_depth, sink)
            {
                eprintln!("warning: failed to read {}: {error}", path.display());
            }
            continue;
        }

        sink(row_for_path(target, &path));
    }

    Ok(())
}

fn row_for_path(target: &Path, path: &Path) -> Row {
    let name = path
        .strip_prefix(target)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    let usage = scan_path(path);

    Row { name, usage }
}

fn is_empty_dir(path: &Path) -> bool {
    fs::read_dir(path)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false)
}

fn scan_path(path: &Path) -> Usage {
    let mut usage = Usage::default();
    let mut stack = vec![path.to_path_buf()];

    while let Some(current) = stack.pop() {
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) => {
                eprintln!("warning: failed to read {}: {error}", current.display());
                continue;
            }
        };

        if metadata.is_file() {
            usage.add_file(metadata.len(), metadata.modified().ok());
            continue;
        }

        if !metadata.is_dir() {
            continue;
        }

        let entries = match fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(error) => {
                eprintln!("warning: failed to read {}: {error}", current.display());
                continue;
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    eprintln!(
                        "warning: failed to read an entry under {}: {error}",
                        current.display()
                    );
                    continue;
                }
            };

            stack.push(entry.path());
        }
    }

    usage
}

fn print_header() {
    let terminal_width = env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(100);
    let fixed_width = 14 + 12 + 23 + 23 + 4;
    let name_width = terminal_width.saturating_sub(fixed_width).clamp(8, 40);

    println!(
        "{:<name_width$} {:>14} {:>12} {:<23} {:<23}",
        "PATH", "SIZE", "FILES", "OLDEST", "NEWEST"
    );
}

fn print_row(row: &Row) {
    let terminal_width = env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(100);
    let fixed_width = 14 + 12 + 23 + 23 + 4;
    let name_width = terminal_width.saturating_sub(fixed_width).clamp(8, 40);

    println!(
        "{:<name_width$} {:>14} {:>12} {:<23} {:<23}",
        truncate(&row.name, name_width),
        format_bytes(row.usage.bytes),
        row.usage.files,
        format_time(row.usage.oldest),
        format_time(row.usage.newest)
    );
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }

    let mut output: String = value.chars().take(width.saturating_sub(3)).collect();
    output.push_str("...");
    output
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;

    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn format_time(time: Option<SystemTime>) -> String {
    let Some(time) = time else {
        return "-".to_string();
    };

    let Ok(duration) = time.duration_since(UNIX_EPOCH) else {
        return "before epoch".to_string();
    };

    let Ok(seconds) = i64::try_from(duration.as_secs()) else {
        return "out of range".to_string();
    };

    let mut local = Tm::default();
    #[cfg(unix)]
    let local = unsafe {
        let seconds = seconds as c_long;
        if localtime_r(&seconds, &mut local).is_null() {
            return "invalid time".to_string();
        }
        local
    };
    #[cfg(windows)]
    let local = unsafe {
        if _localtime64_s(&mut local, &seconds) != 0 {
            return "invalid time".to_string();
        }
        local
    };

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        local.tm_year + 1900,
        local.tm_mon + 1,
        local.tm_mday,
        local.tm_hour,
        local.tm_min,
        local.tm_sec
    )
}
