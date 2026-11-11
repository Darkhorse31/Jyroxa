//! Local patch of `windows-link`.
//!
//! Upstream emits `#[link(..., kind = "raw-dylib")]`, which makes rustc build
//! import libraries with `dlltool` at link time. On this host the GNU `dlltool`
//! cannot run (no bundled assembler) and LLVM's dlltool produces import tables
//! that fault at runtime. Instead we link the classic way (`kind = "dylib"`,
//! `+verbatim`) against the prebuilt MinGW import archives shipped with the
//! toolchain, copied under their verbatim DLL names onto the link search path
//! (see `.cargo/config.toml` and `.toolbin/winlibs`). These import libraries
//! are correct, so the resulting binaries run.
#![no_std]

/// Defines an external function to import.
#[cfg(all(windows, target_arch = "x86"))]
#[macro_export]
macro_rules! link {
    ($library:literal $abi:literal $($link_name:literal)? fn $($function:tt)*) => (
        #[link(name = $library, kind = "dylib", modifiers = "+verbatim", import_name_type = "undecorated")]
        extern $abi {
            $(#[link_name=$link_name])?
            pub fn $($function)*;
        }
    )
}

/// Defines an external function to import.
#[cfg(all(windows, not(target_arch = "x86")))]
#[macro_export]
macro_rules! link {
    ($library:literal $abi:literal $($link_name:literal)? fn $($function:tt)*) => (
        #[link(name = $library, kind = "dylib", modifiers = "+verbatim")]
        extern $abi {
            $(#[link_name=$link_name])?
            pub fn $($function)*;
        }
    )
}

/// Defines an external function to import.
#[cfg(not(windows))]
#[macro_export]
macro_rules! link {
    ($library:literal $abi:literal $($link_name:literal)? fn $($function:tt)*) => (
        extern $abi {
            pub fn $($function)*;
        }
    )
}
