use crate::tokenizer::{Decoder, Offsets, PreTokenizer, Result};
use regex::Regex;
use std::collections::HashMap;
use unicode_categories::UnicodeCategories;

fn bytes_char() -> HashMap<u8, u32> {
    let mut bs: Vec<u8> = vec![];
    bs.extend(b'!'..=b'~');
    bs.extend(b'\xA1'..=b'\xAC');
    bs.extend(b'\xAE'..=b'\xFF');

    let mut cs: Vec<u32> = bs.iter().map(|i| *i as u32).collect();
    let mut n = 0;

    for b in 0..=255u8 {
        if !bs.contains(&b) {
            bs.push(b);
            cs.push(u32::pow(2, 8) + n);
            n += 1;
        }
    }

    bs.into_iter().zip(cs).collect()
}

lazy_static! {
    static ref RE: Regex =
        Regex::new(r"'s|'t|'re|'ve|'m|'ll|'d| ?\p{L}+| ?\p{N}+| ?[^\s\p{L}\p{N}]+|\s+").unwrap();
    static ref BYTES_CHAR: HashMap<u8, u32> = bytes_char();
    static ref CHAR_BYTES: HashMap<u32, u8> =
        bytes_char().into_iter().map(|(c, b)| (b, c)).collect();
}

pub struct ByteLevel {
    add_prefix_space: bool,
}
impl ByteLevel {
    pub fn new(add_prefix_space: bool) -> Self {
        ByteLevel { add_prefix_space }
    }
}

impl PreTokenizer for ByteLevel {
    fn pre_tokenize(&self, s: &str) -> Result<Vec<(String, Offsets)>> {
        let s = if self.add_prefix_space && !s.starts_with(' ') {
            format!(" {}", s)
        } else {
            s.to_owned()
        };

        Ok(RE
            .captures_iter(&s)
            .map(|capture| {
                let capture = capture.get(0).unwrap();
                let start = capture.start();
                let end = capture.end();

                // if our last character is a whitespace, followed by a non whitespace,
                // we don't want to return it
                let last = s[start..end].chars().last();
                let next = s[end..].chars().nth(0);
                if let (Some(last), Some(next)) = (last, next) {
                    if last.is_separator_space() && !next.is_separator_space() {
                        if let Some((_last, others)) =
                            s[start..end].chars().collect::<Vec<_>>().split_last()
                        {
                            let bytes = others.iter().collect::<String>().as_bytes().to_vec();
                            let offsets = (start, end - 1);
                            return (bytes, offsets);
                        }
                    }
                }
                // if our first char is not a whitespace but the previous one was, we return
                // a whitespace before our match
                let prev = s[0..start].chars().last();
                let current = s[start..end].chars().nth(0).map(|c| c.is_whitespace());
                if let (Some(prev), Some(current)) = (prev, current) {
                    if prev.is_separator_space() && !current {
                        let bytes =
                            [format!("{}", prev).as_bytes(), s[start..end].as_bytes()].concat();
                        let offsets = (start - prev.len_utf8(), end);
                        return (bytes, offsets);
                    }
                }

                (s[start..end].as_bytes().to_vec(), (start, end))
            })
            .map(|(s, offsets)| {
                (
                    s.iter()
                        .map(|b| std::char::from_u32(BYTES_CHAR[b]).unwrap())
                        .collect(),
                    offsets,
                )
            })
            .collect())
    }
}

impl Decoder for ByteLevel {
    fn decode(&self, tokens: Vec<String>) -> Result<String> {
        Ok(String::from_utf8_lossy(
            &tokens
                .join("")
                .chars()
                .map(|c| CHAR_BYTES[&(c as u32)])
                .collect::<Vec<_>>(),
        )
        .into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::ByteLevel;
    use crate::tokenizer::{Decoder, PreTokenizer};

    #[test]
    fn pre_tokenization() {
        let pre_tok = ByteLevel::new(false);
        assert_eq!(
            pre_tok
                .pre_tokenize("Hello my friend, how is your day going?")
                .unwrap(),
            vec![
                ("Hello".into(), (0, 5)),
                ("??my".into(), (5, 8)),
                ("??friend".into(), (8, 15)),
                (",".into(), (15, 16)),
                ("??how".into(), (16, 20)),
                ("??is".into(), (20, 23)),
                ("??your".into(), (23, 28)),
                ("??day".into(), (28, 32)),
                ("??going".into(), (32, 38)),
                ("?".into(), (38, 39))
            ]
        );
    }

    #[test]
    fn decoding() {
        let decoder = ByteLevel::new(false);
        assert_eq!(
            "Hello my friend, how is your day going?",
            decoder
                .decode(
                    vec![
                        "Hello", "??my", "??friend", ",", "??how", "??is", "??your", "??day", "??going",
                        "?"
                    ]
                    .into_iter()
                    .map(|s| s.into())
                    .collect::<Vec<String>>()
                )
                .unwrap()
        );
    }

    #[test]
    fn add_prefix_space() {
        let pre_tok = ByteLevel::new(true);
        assert_eq!(
            pre_tok
                .pre_tokenize("Hello my friend, how is your day going?")
                .unwrap(),
            vec![
                ("??Hello".into(), (0, 6)),
                ("??my".into(), (6, 9)),
                ("??friend".into(), (9, 16)),
                (",".into(), (16, 17)),
                ("??how".into(), (17, 21)),
                ("??is".into(), (21, 24)),
                ("??your".into(), (24, 29)),
                ("??day".into(), (29, 33)),
                ("??going".into(), (33, 39)),
                ("?".into(), (39, 40))
            ]
        );
    }

    #[test]
    fn decode_works_on_separated_tokens() {
        let samples = vec![
            String::from(
                "A Nuskhuri abbreviation of ???????????? ?????????????????? ( iesu kriste ) \" Jesus Christ \"",
            ),
            String::from(
                "An equal number have descenders , like p or q in English \
                 : ??? , ??? , ??? , ??? , ??? , ??? , ??? , ??? , ??? , ??? , ??? , ??? , ???",
            ),
        ];

        let bl = ByteLevel::new(false);
        for sample in samples {
            let pre_tokenized = bl.pre_tokenize(&sample).unwrap();
            let separated_tokens = pre_tokenized
                .into_iter()
                .map(|(token, _)| token.split("").map(|t| t.into()).collect::<Vec<_>>())
                .flatten()
                .collect::<Vec<_>>();
            assert_eq!(sample, bl.decode(separated_tokens).unwrap());
        }
    }

    #[test]
    fn handling_of_newlines() {
        let s = String::from("Hello there\nHello there");
        let pretok = ByteLevel::new(false);
        let p = pretok.pre_tokenize(&s).unwrap();

        assert_eq!(
            p,
            vec![
                ("Hello".into(), (0, 5)),
                ("??there".into(), (5, 11)),
                ("??".into(), (11, 12)),
                ("Hello".into(), (12, 17)),
                ("??there".into(), (17, 23))
            ]
        );
    }

    #[test]
    fn handling_of_multiple_whitespaces() {
        let s = String::from("Hello there       dear");
        let pretok = ByteLevel::new(false);
        let p = pretok.pre_tokenize(&s).unwrap();

        assert_eq!(
            p,
            vec![
                ("Hello".into(), (0, 5)),
                ("??there".into(), (5, 11)),
                ("????????????".into(), (11, 17)),
                ("??dear".into(), (17, 22))
            ]
        );
    }
}
