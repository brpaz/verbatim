/// A recognised stretch of speech between pauses. Times are milliseconds from
/// the start of the audio.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    /// Word timings, empty when the recogniser does not report them.
    pub words: Vec<Word>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub start_ms: u64,
    pub end_ms: u64,
    /// Includes attached punctuation, as in "Hello,".
    pub text: String,
}

/// "m:ss", or "h:mm:ss" from one hour on, as shown next to transcript lines.
pub fn clock_time(ms: u64) -> String {
    let secs = ms / 1000;
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_time_shows_hours_only_when_needed() {
        assert_eq!(clock_time(0), "0:00");
        assert_eq!(clock_time(65_999), "1:05");
        assert_eq!(clock_time(3_725_000), "1:02:05");
    }
}
