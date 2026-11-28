use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptionConfig {
    pub max_lines: usize,
    pub show_partial: bool,
}

impl Default for CaptionConfig {
    fn default() -> Self {
        Self {
            max_lines: 2,
            show_partial: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentStatus {
    Partial,
    Final,
}

/// A timed recognition segment. Shared by the ASR boundary and caption state so no
/// conversion step can drop the timing.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: Option<u64>,
    pub status: SegmentStatus,
    /// Always `None` today: greedy sherpa decoding does not expose a score.
    pub confidence: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CaptionEvent {
    Partial(String),
    Final { id: u64, segment: Segment },
}

#[derive(Clone, Debug, PartialEq)]
pub struct CaptionLine {
    pub id: u64,
    pub text: String,
    pub start_ms: u64,
    pub end_ms: Option<u64>,
    pub status: SegmentStatus,
    pub confidence: Option<f32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CaptionSnapshot {
    pub partial: String,
    pub finals: Vec<CaptionLine>,
}

pub struct Captions {
    cfg: CaptionConfig,
    partial: String,
    finals: VecDeque<CaptionLine>,
    next_id: u64,
}

impl Captions {
    pub fn new(mut cfg: CaptionConfig) -> Self {
        cfg.max_lines = cfg.max_lines.max(1);
        Self {
            cfg,
            partial: String::new(),
            finals: VecDeque::new(),
            next_id: 1,
        }
    }

    pub fn set_partial(&mut self, text: &str) -> Option<CaptionEvent> {
        if !self.cfg.show_partial {
            return None;
        }
        let text = text.trim();
        if text == self.partial {
            return None;
        }
        self.partial.clear();
        if text.is_empty() {
            return Some(CaptionEvent::Partial(String::new()));
        }
        self.partial.push_str(text);
        Some(CaptionEvent::Partial(self.partial.clone()))
    }

    pub fn add_final(&mut self, segment: &Segment) -> Option<CaptionEvent> {
        let text = segment.text.trim();
        if text.is_empty() {
            return None;
        }
        self.partial.clear();
        let id = self.next_id;
        self.next_id += 1;
        let final_segment = Segment {
            text: text.to_owned(),
            start_ms: segment.start_ms,
            end_ms: segment.end_ms,
            status: SegmentStatus::Final,
            confidence: segment.confidence,
        };
        self.finals.push_back(CaptionLine {
            id,
            text: final_segment.text.clone(),
            start_ms: final_segment.start_ms,
            end_ms: final_segment.end_ms,
            status: final_segment.status,
            confidence: final_segment.confidence,
        });
        while self.finals.len() > self.cfg.max_lines {
            self.finals.pop_front();
        }
        Some(CaptionEvent::Final {
            id,
            segment: final_segment,
        })
    }

    pub fn partial(&self) -> &str {
        &self.partial
    }

    pub fn snapshot(&self) -> CaptionSnapshot {
        CaptionSnapshot {
            partial: self.partial.clone(),
            finals: self.finals.iter().cloned().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn captions(max_lines: usize) -> Captions {
        Captions::new(CaptionConfig {
            max_lines,
            show_partial: true,
        })
    }

    fn segment(text: &str, start_ms: u64, end_ms: u64) -> Segment {
        Segment {
            text: text.into(),
            start_ms,
            end_ms: Some(end_ms),
            status: SegmentStatus::Final,
            confidence: None,
        }
    }

    #[test]
    fn partial_replaces_and_clears() {
        let mut c = captions(2);
        assert_eq!(
            c.set_partial("  hello"),
            Some(CaptionEvent::Partial("hello".into()))
        );
        assert_eq!(
            c.set_partial("hello world"),
            Some(CaptionEvent::Partial("hello world".into()))
        );
        assert_eq!(c.set_partial("hello world"), None);
        assert_eq!(
            c.set_partial(""),
            Some(CaptionEvent::Partial(String::new()))
        );
        assert_eq!(c.set_partial(""), None);
        assert_eq!(c.partial(), "");
    }

    #[test]
    fn final_clears_partial_and_gets_id() {
        let mut c = captions(2);
        c.set_partial("hello");
        assert_eq!(
            c.add_final(&segment("hello there", 100, 900)),
            Some(CaptionEvent::Final {
                id: 1,
                segment: segment("hello there", 100, 900),
            })
        );
        assert_eq!(c.partial(), "");
        assert_eq!(c.snapshot().partial, "");
        assert_eq!(c.snapshot().finals.len(), 1);
    }

    #[test]
    fn partial_then_final_does_not_duplicate_final() {
        let mut c = captions(3);
        c.set_partial("partial text");
        assert!(c.add_final(&segment("final text", 10, 20)).is_some());
        assert_eq!(c.snapshot().finals.len(), 1);
        assert_eq!(c.snapshot().finals[0].text, "final text");
    }

    #[test]
    fn timestamps_and_status_survive_snapshot() {
        let mut c = captions(2);
        c.add_final(&segment("timed", 1234, 5678));
        let line = &c.snapshot().finals[0];
        assert_eq!(line.start_ms, 1234);
        assert_eq!(line.end_ms, Some(5678));
        assert_eq!(line.status, SegmentStatus::Final);
        assert_eq!(line.confidence, None);
    }

    #[test]
    fn prune_keeps_newest_with_timings() {
        let mut c = captions(2);
        c.add_final(&segment("one", 0, 10));
        c.add_final(&segment("two", 10, 20));
        c.add_final(&segment("three", 20, 30));
        let finals = c.snapshot().finals;
        assert_eq!(finals.iter().map(|l| l.id).collect::<Vec<_>>(), vec![2, 3]);
        assert_eq!(finals[0].start_ms, 10);
        assert_eq!(finals[1].start_ms, 20);
    }

    #[test]
    fn max_lines_prunes_oldest() {
        let mut c = captions(2);
        c.add_final(&segment("one", 0, 1));
        c.add_final(&segment("two", 1, 2));
        c.add_final(&segment("three", 2, 3));
        let ids: Vec<u64> = c.snapshot().finals.iter().map(|l| l.id).collect();
        assert_eq!(ids, vec![2, 3]);
    }

    #[test]
    fn whitespace_final_ignored() {
        let mut c = captions(2);
        assert!(c.add_final(&segment("   ", 0, 1)).is_none());
        assert!(c.add_final(&segment("", 0, 1)).is_none());
        assert!(c.snapshot().finals.is_empty());
    }

    #[test]
    fn show_partial_false_emits_nothing() {
        let mut c = Captions::new(CaptionConfig {
            max_lines: 2,
            show_partial: false,
        });
        assert_eq!(c.set_partial("hello"), None);
        assert_eq!(c.partial(), "");
        assert_eq!(
            c.add_final(&segment("final", 0, 1)),
            Some(CaptionEvent::Final {
                id: 1,
                segment: segment("final", 0, 1),
            })
        );
    }

    #[test]
    fn max_lines_is_at_least_one() {
        let mut c = captions(0);
        c.add_final(&segment("one", 0, 1));
        c.add_final(&segment("two", 1, 2));
        assert_eq!(c.snapshot().finals.len(), 1);
        assert_eq!(c.snapshot().finals[0].text, "two");
    }
}
