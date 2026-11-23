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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptionEvent {
    Partial(String),
    Final { id: u64, text: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptionLine {
    pub id: u64,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
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

    pub fn add_final(&mut self, text: &str) -> Option<CaptionEvent> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        self.partial.clear();
        let id = self.next_id;
        self.next_id += 1;
        self.finals.push_back(CaptionLine {
            id,
            text: text.to_string(),
        });
        while self.finals.len() > self.cfg.max_lines {
            self.finals.pop_front();
        }
        Some(CaptionEvent::Final {
            id,
            text: text.to_string(),
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
            c.add_final("hello there"),
            Some(CaptionEvent::Final {
                id: 1,
                text: "hello there".into()
            })
        );
        assert_eq!(c.partial(), "");
        assert_eq!(c.snapshot().partial, "");
        assert_eq!(c.snapshot().finals.len(), 1);
    }

    #[test]
    fn max_lines_prunes_oldest() {
        let mut c = captions(2);
        c.add_final("one");
        c.add_final("two");
        c.add_final("three");
        let ids: Vec<u64> = c.snapshot().finals.iter().map(|l| l.id).collect();
        assert_eq!(ids, vec![2, 3]);
    }

    #[test]
    fn whitespace_final_ignored() {
        let mut c = captions(2);
        assert_eq!(c.add_final("   "), None);
        assert_eq!(c.add_final(""), None);
        assert_eq!(c.snapshot().finals.len(), 0);
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
            c.add_final("final"),
            Some(CaptionEvent::Final {
                id: 1,
                text: "final".into()
            })
        );
    }

    #[test]
    fn max_lines_is_at_least_one() {
        let mut c = captions(0);
        c.add_final("one");
        c.add_final("two");
        assert_eq!(c.snapshot().finals.len(), 1);
        assert_eq!(c.snapshot().finals[0].text, "two");
    }
}
