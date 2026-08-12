#![allow(unused)]

/// Gives each name its position in the list as a `u32` constant.
macro_rules! define_type_indices {
    ($($name:ident),*$(,)?) => {
        define_type_indices!(@step 0u32, $($name,)*);
    };
    (@step $index:expr,) => {};
    (@step $index:expr, $head:ident, $($tail:ident,)*) => {
        pub(crate) const $head: u32 = $index;
        define_type_indices!(@step $index + 1u32, $($tail,)*);
    };
}

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

        /// Position of each type in [`super::SUPPORTED_TYPES`], carried by an encoded token.
        pub(crate) mod idx {
            define_type_indices!($($standard,)* $($custom,)*);
        }

        #[cfg(test)]
        mod idx_tests {
            use super::*;

            #[test]
            fn indices_match_the_legend() {
                $(assert_eq!(SUPPORTED_TYPES[idx::$standard as usize], $standard);)*
                $(assert_eq!(SUPPORTED_TYPES[idx::$custom as usize], $custom);)*
            }
        }

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
        CLASS,
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
