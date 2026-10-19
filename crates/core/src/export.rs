use std::fmt::Write;

use crate::transcript::{Segment, Word};

/// Characters per subtitle line, and lines per cue, as in common subtitle
/// style guides.
const LINE_CHARS: usize = 42;
const CUE_LINES: usize = 2;
const CUE_MAX_MS: u64 = 7_000;
/// A sentence ends a cue only once the cue holds this much, so short replies
/// like "Yes." do not flash by on their own.
const SENTENCE_BREAK_CHARS: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// One paragraph per segment, without timestamps.
    Text,
    Srt,
    WebVtt,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Text => "txt",
            Format::Srt => "srt",
            Format::WebVtt => "vtt",
        }
    }
}

pub fn render(segments: &[Segment], format: Format) -> String {
    match format {
        Format::Text => segments
            .iter()
            .map(|segment| format!("{}\n", segment.text))
            .collect::<Vec<_>>()
            .join("\n"),
        Format::Srt => {
            let mut out = String::new();
            for (i, cue) in cues(segments).iter().enumerate() {
                let _ = write!(
                    out,
                    "{}\n{} --> {}\n{}\n\n",
                    i + 1,
                    subtitle_time(cue.start_ms, ','),
                    subtitle_time(cue.end_ms, ','),
                    cue.text
                );
            }
            out
        }
        Format::WebVtt => {
            let mut out = String::from("WEBVTT\n\n");
            for cue in cues(segments) {
                let _ = write!(
                    out,
                    "{} --> {}\n{}\n\n",
                    subtitle_time(cue.start_ms, '.'),
                    subtitle_time(cue.end_ms, '.'),
                    cue.text
                );
            }
            out
        }
    }
}

#[derive(Debug, PartialEq)]
struct Cue {
    start_ms: u64,
    end_ms: u64,
    /// One or two lines.
    text: String,
}

/// Splits segments into cues short enough to read. Cues never span two
/// segments, since a pause separates them.
fn cues(segments: &[Segment]) -> Vec<Cue> {
    let mut cues = Vec::new();
    for segment in segments {
        if segment.words.is_empty() {
            cues.push(Cue {
                start_ms: segment.start_ms,
                end_ms: segment.end_ms,
                text: wrap(&segment.text),
            });
            continue;
        }
        let mut current: Vec<&Word> = Vec::new();
        for word in &segment.words {
            if let Some(first) = current.first() {
                let len = joined_len(&current);
                let mut candidate = current.clone();
                candidate.push(word);
                let full = line_count(&candidate) > CUE_LINES
                    || word.end_ms.saturating_sub(first.start_ms) > CUE_MAX_MS;
                let sentence_end = len >= SENTENCE_BREAK_CHARS
                    && current
                        .last()
                        .is_some_and(|last| last.text.ends_with(['.', '?', '!']));
                if full || sentence_end {
                    cues.push(cue(&current));
                    current.clear();
                }
            }
            current.push(word);
        }
        if !current.is_empty() {
            cues.push(cue(&current));
        }
    }
    cues
}

fn cue(words: &[&Word]) -> Cue {
    let text = words
        .iter()
        .map(|word| word.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    Cue {
        start_ms: words[0].start_ms,
        end_ms: words[words.len() - 1].end_ms,
        text: wrap(&text),
    }
}

fn joined_len(words: &[&Word]) -> usize {
    words.iter().map(|word| chars(&word.text)).sum::<usize>() + words.len().saturating_sub(1)
}

/// Lines the words take when each line is filled as far as it goes.
fn line_count(words: &[&Word]) -> usize {
    let mut lines = 0;
    let mut line = 0;
    for word in words {
        let len = chars(&word.text);
        if lines == 0 || line + 1 + len > LINE_CHARS {
            lines += 1;
            line = len;
        } else {
            line += 1 + len;
        }
    }
    lines
}

fn chars(text: &str) -> usize {
    text.chars().count()
}

/// Breaks text longer than a line in two, as evenly as possible while
/// keeping both lines within the limit when the text allows it.
fn wrap(text: &str) -> String {
    if chars(text) <= LINE_CHARS {
        return text.to_owned();
    }
    let longest_line = |i: usize| chars(&text[..i]).max(chars(&text[i + 1..]));
    match text
        .match_indices(' ')
        .map(|(i, _)| i)
        .min_by_key(|&i| longest_line(i))
    {
        Some(i) => format!("{}\n{}", &text[..i], &text[i + 1..]),
        None => text.to_owned(),
    }
}

/// "hh:mm:ss,mmm" for SRT, "hh:mm:ss.mmm" for WebVTT.
fn subtitle_time(ms: u64, separator: char) -> String {
    let (h, m, s, ms) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
    format!("{h:02}:{m:02}:{s:02}{separator}{ms:03}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A segment whose words each last 300 ms, back to back.
    fn segment(start_ms: u64, text: &str) -> Segment {
        let words: Vec<Word> = text
            .split(' ')
            .enumerate()
            .map(|(i, word)| Word {
                start_ms: start_ms + i as u64 * 300,
                end_ms: start_ms + (i as u64 + 1) * 300,
                text: word.to_owned(),
            })
            .collect();
        Segment {
            start_ms,
            end_ms: words.last().unwrap().end_ms,
            text: text.to_owned(),
            words,
        }
    }

    #[test]
    fn text_has_one_paragraph_per_segment() {
        let segments = [segment(0, "Hello there."), segment(5_000, "How are you?")];

        assert_eq!(
            render(&segments, Format::Text),
            "Hello there.\n\nHow are you?\n"
        );
    }

    #[test]
    fn srt_numbers_cues_and_uses_commas_in_times() {
        let segments = [segment(1_000, "Hello there."), segment(3_723_456, "Bye.")];

        assert_eq!(
            render(&segments, Format::Srt),
            "1\n00:00:01,000 --> 00:00:01,600\nHello there.\n\n\
             2\n01:02:03,456 --> 01:02:03,756\nBye.\n\n"
        );
    }

    #[test]
    fn webvtt_has_header_and_uses_dots_in_times() {
        let segments = [segment(1_000, "Hello there.")];

        assert_eq!(
            render(&segments, Format::WebVtt),
            "WEBVTT\n\n00:00:01.000 --> 00:00:01.600\nHello there.\n\n"
        );
    }

    #[test]
    fn long_segment_splits_into_cues_of_at_most_two_lines() {
        let text = "This is a rather long stretch of speech that goes on and on \
                    without any pause at all because the speaker never stops talking";
        let cues = cues(&[segment(0, text)]);

        assert!(cues.len() > 1);
        for cue in &cues {
            let lines: Vec<_> = cue.text.lines().collect();
            assert!(lines.len() <= 2, "{cue:?}");
            assert!(lines.iter().all(|line| line.len() <= LINE_CHARS), "{cue:?}");
        }
        let words: Vec<_> = cues
            .iter()
            .flat_map(|cue| cue.text.split_whitespace())
            .collect();
        assert_eq!(words.join(" "), text);
    }

    #[test]
    fn cue_never_lasts_longer_than_the_limit() {
        let slow: Vec<Word> = (0..10)
            .map(|i| Word {
                start_ms: i * 1_000,
                end_ms: i * 1_000 + 900,
                text: "word".into(),
            })
            .collect();
        let segment = Segment {
            start_ms: 0,
            end_ms: 9_900,
            text: ["word"; 10].join(" "),
            words: slow,
        };

        let cues = cues(&[segment]);

        assert!(
            cues.iter()
                .all(|cue| cue.end_ms - cue.start_ms <= CUE_MAX_MS)
        );
        assert_eq!(cues.len(), 2);
    }

    #[test]
    fn sentence_ends_a_cue_once_it_is_long_enough() {
        let cues = cues(&[segment(0, "Yes. I think that is right, really. And then")]);

        let texts: Vec<_> = cues.iter().map(|cue| cue.text.as_str()).collect();
        assert_eq!(texts, ["Yes. I think that is right, really.", "And then"]);
    }

    #[test]
    fn segment_without_words_becomes_one_wrapped_cue() {
        let segment = Segment {
            start_ms: 0,
            end_ms: 4_000,
            text: "A segment with no word timings that is longer than a line".into(),
            words: vec![],
        };

        let cues = cues(&[segment]);

        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].text.lines().count(), 2);
        assert_eq!((cues[0].start_ms, cues[0].end_ms), (0, 4_000));
    }
}
