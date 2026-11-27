/// Declares an enum that round-trips through a stable snake_case id.
macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $id:literal, $label:literal);+ $(;)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub enum $name {
            $(#[serde(rename = $id)] $variant),+
        }

        impl $name {
            /// Every variant, in display order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The stable id stored in SQLite and sent over the wire.
            pub const fn as_str(self) -> &'static str {
                match self { $($name::$variant => $id),+ }
            }

            /// The human label from the prototype.
            pub const fn label(self) -> &'static str {
                match self { $($name::$variant => $label),+ }
            }

            /// Parse a stored id; unknown values are rejected.
            pub fn parse(value: &str) -> Result<Self> {
                match value {
                    $($id => Ok($name::$variant),)+
                    other => Err(CoreError::UnknownEnum {
                        enum_name: stringify!($name),
                        value: other.to_string(),
                    }),
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = CoreError;
            fn from_str(value: &str) -> Result<Self> { Self::parse(value) }
        }
    };
}
