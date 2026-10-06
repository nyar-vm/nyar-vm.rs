//! 二进制格式模型（过渡 `vcc-data` → `acorn-*`）。

//!

//! JVM `class` / `JAR` 已迁入 `acorn-jvm`。

//! `COFF` / `ELF` / 原生 `PE` 探测已迁入 `acorn-pe`。

//! `CLR` `PE` 元数据写入仍经 `vcc-data`。



pub use acorn_jvm::{class, jar};

pub use acorn_pe::{coff, elf};



/// `PE` 面：`acorn-pe` 负责探测与原生写出，`vcc-data` 仍提供 `CLR` 写入器。

pub mod pe {

    pub use acorn_pe::pe::{

        CliHeader, DataDirectory, DosHeader, MetadataRoot, NativeDllImport, NativePeImage, NativePeWriter, OptionalHeader,

        Pe64Header, Pe64ParseError, PeCoffHeader, PeImage, PeParseError, SectionHeader, StreamHeader, TableKind, extract_pe_section,

        parse_pe, parse_pe64, read_blob, read_compressed_uint, read_strings_string, read_user_string, rva_to_offset,

    };

    pub use vcc_data::binary::pe::{ClrMetadataBuilder, ClrMetadataError, PeWriter, PeWriterError, PeWriterOptions};

}


