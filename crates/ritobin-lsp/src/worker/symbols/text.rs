use ltk_ritobin::{ast::Value, parse::Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Text<S = String> {
    Span(Span),
    Str(S),
}

impl Text<String> {
    pub fn value_type(text: &str, v: &Value) -> Option<Self> {
        Some(Text::Str(match v {
            Value::Struct(object) | Value::Embedded(object) => format!(
                "{}[{}]",
                match v {
                    Value::Struct(_) => "pointer",
                    Value::Embedded(_) => "embed",
                    _ => unreachable!(),
                },
                &text[object.class_hash.span()]
            ),
            _ => return v.rito_type().map(|t| t.to_string().into()),
        }))
    }
}

//

impl<S> From<Span> for Text<S> {
    fn from(value: Span) -> Self {
        Self::Span(value)
    }
}

impl From<String> for Text<String> {
    fn from(value: String) -> Self {
        Self::Str(value)
    }
}
impl<'a> From<&'a str> for Text<&'a str> {
    fn from(value: &'a str) -> Self {
        Self::Str(value)
    }
}
impl<'a> From<&'a str> for Text<String> {
    fn from(value: &'a str) -> Self {
        Self::Str(value.into())
    }
}

impl<S: Into<String>> Text<S> {
    pub fn into_string(self, text: &str) -> String {
        match self {
            Text::Span(span) => text[span].to_string(),
            Text::Str(s) => s.into(),
        }
    }
}
