use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{NaiveDate, Utc};

use crate::config::ensure_zorai_data_dir;

fn normalize_log_stem(file_name: &str) -> &str {
    file_name.strip_suffix(".log").unwrap_or(file_name)
}

pub fn dated_log_file_name(file_name: &str, date: NaiveDate) -> String {
    format!(
        "{}-{}.log",
        normalize_log_stem(file_name),
        date.format("%Y-%m-%d")
    )
}

pub fn dated_log_file_path(
    directory: impl AsRef<Path>,
    file_name: &str,
    date: NaiveDate,
) -> PathBuf {
    directory
        .as_ref()
        .join(dated_log_file_name(file_name, date))
}

struct WriterState {
    current_date: NaiveDate,
    file: File,
}

pub struct DailyLogWriter {
    log_dir: PathBuf,
    file_name: String,
    date_provider: Box<dyn Fn() -> NaiveDate + Send + Sync>,
    state: Mutex<WriterState>,
}

impl DailyLogWriter {
    pub fn new(file_name: &str) -> io::Result<Self> {
        Self::new_in_directory_with_clock(ensure_zorai_data_dir()?, file_name, || {
            Utc::now().date_naive()
        })
    }

    pub fn new_in_directory(directory: impl AsRef<Path>, file_name: &str) -> io::Result<Self> {
        Self::new_in_directory_with_clock(directory, file_name, || Utc::now().date_naive())
    }

    pub fn new_in_directory_with_clock(
        directory: impl AsRef<Path>,
        file_name: &str,
        date_provider: impl Fn() -> NaiveDate + Send + Sync + 'static,
    ) -> io::Result<Self> {
        let log_dir = directory.as_ref().to_path_buf();
        std::fs::create_dir_all(&log_dir)?;
        let initial_date = date_provider();
        let file = open_dated_log_file(&log_dir, file_name, initial_date)?;

        Ok(Self {
            log_dir,
            file_name: file_name.to_string(),
            date_provider: Box::new(date_provider),
            state: Mutex::new(WriterState {
                current_date: initial_date,
                file,
            }),
        })
    }

    pub fn current_path(&self) -> io::Result<PathBuf> {
        let state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("log writer mutex poisoned"))?;
        Ok(dated_log_file_path(
            &self.log_dir,
            &self.file_name,
            state.current_date,
        ))
    }

    fn rotate_if_needed(&self, state: &mut WriterState) -> io::Result<()> {
        let current_date = (self.date_provider)();
        if current_date == state.current_date {
            return Ok(());
        }

        state.file = open_dated_log_file(&self.log_dir, &self.file_name, current_date)?;
        state.current_date = current_date;
        Ok(())
    }
}

impl Write for DailyLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("log writer mutex poisoned"))?;
        self.rotate_if_needed(&mut state)?;
        state.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("log writer mutex poisoned"))?;
        self.rotate_if_needed(&mut state)?;
        state.file.flush()
    }
}

fn open_dated_log_file(log_dir: &Path, file_name: &str, date: NaiveDate) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(dated_log_file_path(log_dir, file_name, date))
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    use chrono::NaiveDate;

    use super::*;

    #[test]
    fn dated_log_file_name_uses_hyphenated_date_before_extension() {
        let date = NaiveDate::from_ymd_opt(2026, 4, 2).expect("valid date");

        let file_name = dated_log_file_name("zorai-daemon.log", date);

        assert_eq!(file_name, "zorai-daemon-2026-04-02.log");
        assert_ne!(file_name, "zorai-daemon.log.2026-04-02");
        assert_ne!(file_name, "zorai-daemon.log2026-04-02");
    }

    #[test]
    fn daily_log_writer_rolls_over_when_date_changes() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let dates = Arc::new(Mutex::new(vec![
            NaiveDate::from_ymd_opt(2026, 4, 3).expect("valid date"),
            NaiveDate::from_ymd_opt(2026, 4, 2).expect("valid date"),
            NaiveDate::from_ymd_opt(2026, 4, 3).expect("valid date"),
        ]));
        let clock = {
            let dates = Arc::clone(&dates);
            move || dates.lock().expect("clock").remove(0)
        };

        let mut writer =
            DailyLogWriter::new_in_directory_with_clock(tempdir.path(), "zorai-daemon.log", clock)
                .expect("writer");

        writer.write_all(b"first line\n").expect("first write");
        writer.write_all(b"second line\n").expect("second write");

        let day_one = tempdir.path().join("zorai-daemon-2026-04-02.log");
        let day_two = tempdir.path().join("zorai-daemon-2026-04-03.log");

        assert_eq!(
            std::fs::read_to_string(day_one).expect("day one"),
            "first line\n"
        );
        assert_eq!(
            std::fs::read_to_string(day_two).expect("day two"),
            "second line\n"
        );
    }

    #[test]
    fn dated_log_file_path_roots_in_given_directory() {
        let date = NaiveDate::from_ymd_opt(2026, 4, 2).expect("valid date");
        let dir = std::path::Path::new("/tmp/zorai-tests");

        let path = dated_log_file_path(dir, "zorai-gateway.log", date);

        assert_eq!(path, dir.join("zorai-gateway-2026-04-02.log"));
    }
}
