macro_rules! define_semantic_token_types {
    (
        standard {
            $($standard:ident),*$(,)?
        }
        custom {
            $(($custom:ident, $string:literal) $(=> $fallback:ident)?),*$(,)?
        }

    ) => {
        // pub(crate) mod types {
            use super::SemanticTokenType;
            $(pub(crate) const $standard: SemanticTokenType = SemanticTokenType::$standard;)*
            $(pub(crate) const $custom: SemanticTokenType = SemanticTokenType::new($string);)*
        // }

        pub(crate) const SUPPORTED_TYPES: &[SemanticTokenType] = &[
            $(self::$standard,)*
            $(self::$custom),*
        ];

        pub(crate) fn standard_fallback_type(token: SemanticTokenType) -> Option<SemanticTokenType> {
            use self::*;
            $(
                if token == $custom {
                    None $(.or(Some(self::$fallback)))?
                } else
            )*
            { Some(token )}
        }
    };
}

define_semantic_token_types![
    standard {
        COMMENT,
        DECORATOR,
        ENUM_MEMBER,
        ENUM,
        KEYWORD,
        METHOD,
        NAMESPACE,
        NUMBER,
        OPERATOR,
        PARAMETER,
        PROPERTY,
        STRING,
        STRUCT,
        TYPE_PARAMETER,
        VARIABLE,
        TYPE,
    }

    custom {
        (BOOLEAN, "boolean"),
        (BRACE, "brace"),
        (BRACKET, "bracket"),
        (BUILTIN_TYPE, "builtinType") => TYPE,
        (COLON, "colon"),
        (ESCAPE_SEQUENCE, "escapeSequence") => STRING,
        (INVALID_ESCAPE_SEQUENCE, "invalidEscapeSequence") => STRING,
        (PUNCTUATION, "punctuation"),
        (UNRESOLVED_REFERENCE, "unresolvedReference"),
    }
];

macro_rules! count_tts {
    () => {0usize};
    ($_head:tt $($tail:tt)*) => {1usize + count_tts!($($tail)*)};
}
macro_rules! define_semantic_token_modifiers {
    (
        standard {
            $($standard:ident),*$(,)?
        }
        custom {
            $(($custom:ident, $string:literal)),*$(,)?
        }

    ) => {
        pub(crate) mod modifiers {
            use super::super::SemanticTokenModifier;

            $(pub(crate) const $standard: SemanticTokenModifier = SemanticTokenModifier::$standard;)*
            $(pub(crate) const $custom: SemanticTokenModifier = SemanticTokenModifier::new($string);)*
        }

        use super::SemanticTokenModifier;

        pub(crate) const SUPPORTED_MODIFIERS: &[SemanticTokenModifier] = &[
            $(SemanticTokenModifier::$standard,)*
            $(self::modifiers::$custom),*
        ];

        pub(super) const LAST_STANDARD_MOD: usize = count_tts!($($standard)*);
    };
}

define_semantic_token_modifiers![
    standard {
        DOCUMENTATION,
        DECLARATION,
        STATIC,
        DEFAULT_LIBRARY,
        DEPRECATED,
    }
    custom {
        (CALLABLE, "callable"),
        (CONSTANT, "constant"),
        (INTRA_DOC_LINK, "intraDocLink"),
        (LIBRARY, "library"),
    }
];
