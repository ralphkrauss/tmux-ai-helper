const MAX_OSC_BYTES: usize = 4096;

#[derive(Default)]
pub struct OscParser {
    state: ParserState,
    buffer: Vec<u8>,
}

#[derive(Default)]
enum ParserState {
    #[default]
    Ground,
    Escape,
    Osc,
    OscEscape,
}

impl OscParser {
    pub fn feed(&mut self, input: &[u8]) -> Vec<Vec<u8>> {
        let mut sequences = Vec::new();

        for byte in input {
            match self.state {
                ParserState::Ground => {
                    if *byte == 0x1b {
                        self.state = ParserState::Escape;
                    }
                }
                ParserState::Escape => {
                    if *byte == b']' {
                        self.buffer.clear();
                        self.state = ParserState::Osc;
                    } else if *byte != 0x1b {
                        self.state = ParserState::Ground;
                    }
                }
                ParserState::Osc => match *byte {
                    0x07 => {
                        sequences.push(std::mem::take(&mut self.buffer));
                        self.state = ParserState::Ground;
                    }
                    0x1b => {
                        self.state = ParserState::OscEscape;
                    }
                    _ => self.push_osc_byte(*byte),
                },
                ParserState::OscEscape => {
                    if *byte == b'\\' {
                        sequences.push(std::mem::take(&mut self.buffer));
                        self.state = ParserState::Ground;
                    } else {
                        self.push_osc_byte(0x1b);
                        self.push_osc_byte(*byte);
                        self.state = ParserState::Osc;
                    }
                }
            }
        }

        sequences
    }

    fn push_osc_byte(&mut self, byte: u8) {
        if self.buffer.len() < MAX_OSC_BYTES {
            self.buffer.push(byte);
        } else {
            self.buffer.clear();
            self.state = ParserState::Ground;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bel_terminated_osc() {
        let mut parser = OscParser::default();
        let sequences = parser.feed(b"hello\x1b]9;4;3\x07world");

        assert_eq!(sequences, vec![b"9;4;3".to_vec()]);
    }

    #[test]
    fn parses_st_terminated_osc_across_chunks() {
        let mut parser = OscParser::default();

        assert!(parser.feed(b"\x1b]9;").is_empty());
        let sequences = parser.feed(b"4;1;55\x1b\\");

        assert_eq!(sequences, vec![b"9;4;1;55".to_vec()]);
    }
}
