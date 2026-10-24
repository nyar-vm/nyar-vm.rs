//! `PE` 解析结果的格式化输出。
//!
//! 将 `PeImage` 渲染为人类可读的诊断文本。
//!
//! `PE/CLR` 二进制模型与结构化解码器统一由 `acorn-pe` 提供，
//! 本模块仅保留纯文本渲染函数（`dump_image` / `dump_user_strings` / `dump_blobs`），
//! 以及少量将结构化解码结果转为可读字符串的格式化辅助。

use acorn_pe::pe::{
    CliHeader, CodedIndex, DosHeader, ElementType, MemberRefParent, MetadataRoot, MethodSignature, OptionalHeader, PeCoffHeader as CoffHeader,
    PeImage, SectionHeader, TableKind, coded_index_size, decode_member_ref_parent, decode_method_signature, read_blob, read_compressed_uint,
    read_idx, read_strings_string, read_u32_table, read_user_string, table_data_offset,
};

/// 将 `PE` 镜像格式化为诊断文本。
pub fn dump_image(image: &PeImage) -> String {
    let mut out = String::new();
    dump_dos(&mut out, &image.dos);
    dump_coff(&mut out, &image.coff);
    dump_optional(&mut out, &image.optional);
    dump_sections(&mut out, &image.sections);
    if let Some(cli) = &image.cli {
        dump_cli(&mut out, cli);
    }
    else {
        out.push_str("\n[CLI] 无 CLI 目录（非 CLR 镜像）\n");
    }
    if let Some(md) = &image.metadata {
        dump_metadata(&mut out, md);
    }
    out
}

fn dump_dos(out: &mut String, dos: &DosHeader) {
    out.push_str("=== DOS Header ===\n");
    out.push_str(&format!("  PE offset: 0x{:08X}\n", dos.pe_offset));
}

fn dump_coff(out: &mut String, coff: &CoffHeader) {
    out.push_str("\n=== COFF Header ===\n");
    out.push_str(&format!("  Machine: 0x{:04X} ({})\n", coff.machine, machine_name(coff.machine)));
    out.push_str(&format!("  NumberOfSections: {}\n", coff.number_of_sections));
    out.push_str(&format!("  TimeDateStamp: 0x{:08X}\n", coff.time_date_stamp));
    out.push_str(&format!("  PointerToSymbolTable: 0x{:08X}\n", coff.pointer_to_symbol_table));
    out.push_str(&format!("  NumberOfSymbols: {}\n", coff.number_of_symbols));
    out.push_str(&format!("  SizeOfOptionalHeader: {}\n", coff.size_of_optional_header));
    out.push_str(&format!("  Characteristics: 0x{:04X}\n", coff.characteristics));
}

fn machine_name(machine: u16) -> &'static str {
    match machine {
        0x14C => "I386",
        0x8664 => "AMD64",
        0x1C0 => "ARM",
        0xAA64 => "ARM64",
        _ => "UNKNOWN",
    }
}

fn dump_optional(out: &mut String, opt: &OptionalHeader) {
    let is_pe32plus = opt.magic == 0x20B;
    let label = if is_pe32plus { "PE32+" } else { "PE32" };
    out.push_str(&format!("\n=== Optional Header ({}) ===\n", label));
    out.push_str(&format!("  Magic: 0x{:04X}\n", opt.magic));
    out.push_str(&format!("  LinkerVersion: {}.{}\n", opt.major_linker_version, opt.minor_linker_version));
    out.push_str(&format!("  SizeOfCode: 0x{:08X}\n", opt.size_of_code));
    out.push_str(&format!("  SizeOfInitializedData: 0x{:08X}\n", opt.size_of_initialized_data));
    out.push_str(&format!("  SizeOfUninitializedData: 0x{:08X}\n", opt.size_of_uninitialized_data));
    out.push_str(&format!("  AddressOfEntryPoint: 0x{:08X}\n", opt.address_of_entry_point));
    out.push_str(&format!("  BaseOfCode: 0x{:08X}\n", opt.base_of_code));
    if is_pe32plus {
        out.push_str(&format!("  ImageBase: 0x{:016X}\n", opt.image_base));
    }
    else {
        out.push_str(&format!("  ImageBase: 0x{:08X}\n", opt.image_base as u32));
    }
    out.push_str(&format!("  SectionAlignment: 0x{:08X}\n", opt.section_alignment));
    out.push_str(&format!("  FileAlignment: 0x{:08X}\n", opt.file_alignment));
    out.push_str(&format!("  SizeOfImage: 0x{:08X}\n", opt.size_of_image));
    out.push_str(&format!("  SizeOfHeaders: 0x{:08X}\n", opt.size_of_headers));
    out.push_str(&format!("  Subsystem: {} ({})\n", opt.subsystem, subsystem_name(opt.subsystem)));
    out.push_str(&format!("  DllCharacteristics: 0x{:04X}\n", opt.dll_characteristics));

    out.push_str("\n  Data Directories:\n");
    let dir_names = [
        "Export",
        "Import",
        "Resource",
        "Exception",
        "Certificate",
        "BaseReloc",
        "Debug",
        "Architecture",
        "GlobalPtr",
        "TLS",
        "LoadConfig",
        "BoundImport",
        "IAT",
        "DelayImport",
        "CLR",
        "Reserved",
    ];
    for (i, dd) in opt.data_directories.iter().enumerate() {
        if dd.rva != 0 || dd.size != 0 {
            out.push_str(&format!("    [{:>2}] {:<14} RVA=0x{:08X} Size=0x{:08X}\n", i, dir_names[i], dd.rva, dd.size));
        }
    }
}

fn subsystem_name(subsystem: u16) -> &'static str {
    match subsystem {
        1 => "NATIVE",
        2 => "WINDOWS_GUI",
        3 => "WINDOWS_CUI",
        7 => "POSIX_CUI",
        9 => "WINDOWS_CE_GUI",
        10 => "EFI_APPLICATION",
        11 => "EFI_BOOT_SERVICE_DRIVER",
        12 => "EFI_RUNTIME_DRIVER",
        _ => "UNKNOWN",
    }
}

fn dump_sections(out: &mut String, sections: &[SectionHeader]) {
    out.push_str("\n=== Section Headers ===\n");
    for (i, s) in sections.iter().enumerate() {
        out.push_str(&format!("\n  [{}] {}\n", i, s.name));
        out.push_str(&format!("    VirtualSize:    0x{:08X}\n", s.virtual_size));
        out.push_str(&format!("    VirtualAddress: 0x{:08X}\n", s.virtual_address));
        out.push_str(&format!("    SizeOfRawData:  0x{:08X}\n", s.size_of_raw_data));
        out.push_str(&format!("    PointerToRaw:   0x{:08X}\n", s.pointer_to_raw_data));
        out.push_str(&format!("    Characteristics: 0x{:08X}\n", s.characteristics));
    }
}

fn dump_cli(out: &mut String, cli: &CliHeader) {
    out.push_str("\n=== CLI Header ===\n");
    out.push_str(&format!("  Cb: 0x{:08X}\n", cli.cb));
    out.push_str(&format!("  RuntimeVersion: {}.{}\n", cli.major_runtime_version, cli.minor_runtime_version));
    out.push_str(&format!("  MetadataRVA: 0x{:08X}\n", cli.metadata_rva));
    out.push_str(&format!("  MetadataSize: 0x{:08X}\n", cli.metadata_size));
    out.push_str(&format!("  Flags: 0x{:08X}\n", cli.flags));
    out.push_str(&format!("  EntryPointToken: 0x{:08X}\n", cli.entry_point_token));
    let table = ((cli.entry_point_token >> 24) & 0xFF) as u8;
    let rid = cli.entry_point_token & 0x00FFFFFF;
    out.push_str(&format!("    -> Table 0x{:02X} ({}) Row {}\n", table, token_table_name(table), rid));
}

fn token_table_name(table: u8) -> &'static str {
    match table {
        0x06 => "MethodDef",
        0x04 => "Field",
        0x02 => "TypeDef",
        0x0A => "MemberRef",
        _ => "Unknown",
    }
}

fn dump_metadata(out: &mut String, md: &MetadataRoot) {
    out.push_str("\n=== Metadata Root ===\n");
    out.push_str(&format!("  Version: {} ({}.{})\n", md.version, md.major_version, md.minor_version));
    out.push_str(&format!("  Streams: {}\n", md.streams.len()));
    for s in &md.streams {
        out.push_str(&format!("    {:<10} offset=0x{:08X} size=0x{:08X}\n", s.name, s.offset, s.size));
    }

    out.push_str(&format!(
        r#"
  #Strings size: {}
  #US size: {}
  #GUID size: {}
  #Blob size: {}
"#,
        md.strings.len(),
        md.user_strings.len(),
        md.guid.len(),
        md.blob.len()
    ));

    out.push_str(&format!(
        r#"
  Valid tables: 0x{:016X}
  Sorted tables: 0x{:016X}
"#,
        md.valid_tables, md.sorted_tables
    ));

    out.push_str("\n  Table row counts:\n");
    for i in 0..64u32 {
        if (md.valid_tables >> i) & 1 == 1 {
            let kind = TableKind::from_raw(i as u8).map(|k| format!("{:?}", k)).unwrap_or_else(|| format!("Table_0x{:02X}", i));
            out.push_str(&format!("    0x{:02X} {:<14} {}\n", i, kind, md.row_counts[i as usize]));
        }
    }

    // 尝试解码关键表。
    dump_tables(out, md);
}

/// 解码并输出关键元数据表。
///
/// 使用 `table_data_offset` 精确定位每个目标表的起始偏移，
/// 避免依赖脆弱的顺序 cursor 推进（中间穿插未知表时会错位）。
fn dump_tables(out: &mut String, md: &MetadataRoot) {
    // 计算各堆索引大小。
    let strings_idx_size = if md.strings.len() > 0xFFFF { 4 } else { 2 };
    let guid_idx_size = if md.guid.len() > 0xFFFF { 4 } else { 2 };
    let blob_idx_size = if md.blob.len() > 0xFFFF { 4 } else { 2 };

    let module_rows = md.row_counts[TableKind::Module as usize];
    let typeref_rows = md.row_counts[TableKind::TypeRef as usize];
    let typedef_rows = md.row_counts[TableKind::TypeDef as usize];
    let methoddef_rows = md.row_counts[TableKind::MethodDef as usize];
    let assembly_rows = md.row_counts[TableKind::Assembly as usize];
    let assemblyref_rows = md.row_counts[TableKind::AssemblyRef as usize];

    // Module 表。
    if module_rows > 0 {
        out.push_str("\n  --- Module table ---\n");
        let row_size = 2 + strings_idx_size + 3 * guid_idx_size;
        let cursor = table_data_offset(md, 0x00).unwrap_or(0);
        for i in 0..module_rows {
            let row_start = cursor + (i as usize) * row_size;
            if row_start + row_size > md.tables.len() {
                out.push_str("    (行越界)\n");
                break;
            }
            let generation = u16::from_le_bytes([md.tables[row_start], md.tables[row_start + 1]]);
            let name_idx = read_idx(&md.tables, row_start + 2, strings_idx_size);
            let mvid_idx = read_idx(&md.tables, row_start + 2 + strings_idx_size, guid_idx_size);
            let name = read_strings_string(&md.strings, name_idx);
            out.push_str(&format!("    [{}] Generation={} Name=\"{}\" Mvid=0x{:X}\n", i + 1, generation, name, mvid_idx));
        }
    }

    // TypeRef 表。
    if typeref_rows > 0 {
        out.push_str("\n  --- TypeRef table ---\n");
        let res_scope_idx_size = coded_index_size(md, CodedIndex::ResolutionScope);
        let row_size = res_scope_idx_size + strings_idx_size + strings_idx_size;
        let cursor = table_data_offset(md, 0x01).unwrap_or(0);
        for i in 0..typeref_rows {
            let row_start = cursor + (i as usize) * row_size;
            if row_start + row_size > md.tables.len() {
                out.push_str("    (行越界)\n");
                break;
            }
            let mut off = row_start;
            let res_scope = read_idx(&md.tables, off, res_scope_idx_size);
            off += res_scope_idx_size;
            let name_idx = read_idx(&md.tables, off, strings_idx_size);
            off += strings_idx_size;
            let ns_idx = read_idx(&md.tables, off, strings_idx_size);
            let name = read_strings_string(&md.strings, name_idx);
            let ns = read_strings_string(&md.strings, ns_idx);
            let full = if ns.is_empty() { name.clone() } else { format!("{}.{}", ns, name) };
            out.push_str(&format!("    [{}] ResolutionScope=0x{:X} Name=\"{}\"\n", i + 1, res_scope, full));
        }
    }

    // TypeDef 表。
    if typedef_rows > 0 {
        out.push_str("\n  --- TypeDef table ---\n");
        let field_rows = md.row_counts[TableKind::Field as usize];
        let method_rows = md.row_counts[TableKind::MethodDef as usize];
        let field_list_idx_size = if field_rows > 0xFFFF { 4 } else { 2 };
        let method_list_idx_size = if method_rows > 0xFFFF { 4 } else { 2 };
        // TypeDefOrRef 编码索引：TypeDef | TypeRef | TypeSpec（2 个 tag bit）
        let extends_idx_size = coded_index_size(md, CodedIndex::TypeDefOrRef);
        let row_size = 4 + strings_idx_size + strings_idx_size + extends_idx_size + field_list_idx_size + method_list_idx_size;
        let cursor = table_data_offset(md, 0x02).unwrap_or(0);
        for i in 0..typedef_rows {
            let row_start = cursor + (i as usize) * row_size;
            if row_start + row_size > md.tables.len() {
                out.push_str("    (行越界)\n");
                break;
            }
            let mut off = row_start;
            let flags = read_u32_table(&md.tables, off);
            off += 4;
            let name_idx = read_idx(&md.tables, off, strings_idx_size);
            off += strings_idx_size;
            let ns_idx = read_idx(&md.tables, off, strings_idx_size);
            off += strings_idx_size;
            let extends = read_idx(&md.tables, off, extends_idx_size);
            off += extends_idx_size;
            let field_list = read_idx(&md.tables, off, field_list_idx_size);
            off += field_list_idx_size;
            let method_list = read_idx(&md.tables, off, method_list_idx_size);
            let name = read_strings_string(&md.strings, name_idx);
            let ns = read_strings_string(&md.strings, ns_idx);
            let full = if ns.is_empty() { name.clone() } else { format!("{}.{}", ns, name) };
            out.push_str(&format!(
                "    [{}] Flags=0x{:08X} Extends=0x{:X} FieldList={} MethodList={} Name=\"{}\"\n",
                i + 1,
                flags,
                extends,
                field_list,
                method_list,
                full
            ));
        }
    }

    // MethodDef 表。
    if methoddef_rows > 0 {
        out.push_str("\n  --- MethodDef table ---\n");
        let param_rows = md.row_counts[TableKind::Param as usize];
        let param_idx_size = if param_rows > 0xFFFF { 4 } else { 2 };
        let row_size = 4 + 2 + 2 + strings_idx_size + blob_idx_size + param_idx_size;
        let cursor = table_data_offset(md, 0x06).unwrap_or(0);
        for i in 0..methoddef_rows {
            let row_start = cursor + (i as usize) * row_size;
            if row_start + row_size > md.tables.len() {
                out.push_str("    (行越界)\n");
                break;
            }
            let mut off = row_start;
            let rva = read_u32_table(&md.tables, off);
            off += 4;
            let impl_flags = u16::from_le_bytes([md.tables[off], md.tables[off + 1]]);
            off += 2;
            let flags = u16::from_le_bytes([md.tables[off], md.tables[off + 1]]);
            off += 2;
            let name_idx = read_idx(&md.tables, off, strings_idx_size);
            off += strings_idx_size;
            let sig_idx = read_idx(&md.tables, off, blob_idx_size);
            off += blob_idx_size;
            let param_list = read_idx(&md.tables, off, param_idx_size);
            let name = read_strings_string(&md.strings, name_idx);
            out.push_str(&format!(
                "    [{}] RVA=0x{:08X} ImplFlags=0x{:04X} Flags=0x{:04X} Name=\"{}\" SigBlob={} ParamList={}\n",
                i + 1,
                rva,
                impl_flags,
                flags,
                name,
                sig_idx,
                param_list
            ));
        }
    }

    // MemberRef 表。
    let memberref_rows = md.row_counts[TableKind::MemberRef as usize];
    if memberref_rows > 0 {
        out.push_str("\n  --- MemberRef table ---\n");
        // MemberRefParent 编码索引：TypeDef | TypeRef | ModuleRef | MethodDef | TypeSpec（3 tag bits）
        let class_idx_size = coded_index_size(md, CodedIndex::MemberRefParent);
        let row_size = class_idx_size + strings_idx_size + blob_idx_size;
        let cursor = table_data_offset(md, 0x0A).unwrap_or(0);
        for i in 0..memberref_rows {
            let row_start = cursor + (i as usize) * row_size;
            if row_start + row_size > md.tables.len() {
                out.push_str("    (行越界)\n");
                break;
            }
            let mut off = row_start;
            let class = read_idx(&md.tables, off, class_idx_size);
            off += class_idx_size;
            let name_idx = read_idx(&md.tables, off, strings_idx_size);
            off += strings_idx_size;
            let sig_idx = read_idx(&md.tables, off, blob_idx_size);
            let name = read_strings_string(&md.strings, name_idx);
            let parent = decode_member_ref_parent(class, class_idx_size);
            let sig_blob = read_blob(&md.blob, sig_idx);
            let signature = decode_method_signature(&sig_blob);
            out.push_str(&format!(
                "    [{}] Class=0x{:X} ({}) Name=\"{}\" Signature={} SigBlob=0x{:X}\n",
                i + 1,
                class,
                format_member_ref_parent(&parent),
                name,
                format_method_signature(&signature),
                sig_idx
            ));
        }
    }

    // Assembly 表。
    if assembly_rows > 0 {
        out.push_str("\n  --- Assembly table ---\n");
        let row_size = 4 + 2 + 2 + 2 + 2 + 4 + blob_idx_size + strings_idx_size + strings_idx_size;
        let cursor = table_data_offset(md, 0x20).unwrap_or(0);
        for i in 0..assembly_rows {
            let row_start = cursor + (i as usize) * row_size;
            if row_start + row_size > md.tables.len() {
                out.push_str("    (行越界)\n");
                break;
            }
            let mut off = row_start;
            let hash_alg = read_u32_table(&md.tables, off);
            off += 4;
            let major = u16::from_le_bytes([md.tables[off], md.tables[off + 1]]);
            off += 2;
            let minor = u16::from_le_bytes([md.tables[off], md.tables[off + 1]]);
            off += 2;
            let build = u16::from_le_bytes([md.tables[off], md.tables[off + 1]]);
            off += 2;
            let rev = u16::from_le_bytes([md.tables[off], md.tables[off + 1]]);
            off += 2;
            let flags = read_u32_table(&md.tables, off);
            off += 4;
            let public_key = read_idx(&md.tables, off, blob_idx_size);
            off += blob_idx_size;
            let name_idx = read_idx(&md.tables, off, strings_idx_size);
            off += strings_idx_size;
            let culture_idx = read_idx(&md.tables, off, strings_idx_size);
            let name = read_strings_string(&md.strings, name_idx);
            out.push_str(&format!(
                "    [{}] HashAlg=0x{:08X} Version={}.{}.{}.{} Flags=0x{:08X} Name=\"{}\" PublicKey={} Culture={}\n",
                i + 1,
                hash_alg,
                major,
                minor,
                build,
                rev,
                flags,
                name,
                public_key,
                culture_idx
            ));
        }
    }

    // AssemblyRef 表。
    if assemblyref_rows > 0 {
        out.push_str("\n  --- AssemblyRef table ---\n");
        let row_size = 2 + 2 + 2 + 2 + 4 + blob_idx_size + strings_idx_size + strings_idx_size + blob_idx_size;
        let cursor = table_data_offset(md, 0x23).unwrap_or(0);
        for i in 0..assemblyref_rows {
            let row_start = cursor + (i as usize) * row_size;
            if row_start + row_size > md.tables.len() {
                out.push_str("    (行越界)\n");
                break;
            }
            let mut off = row_start;
            let major = u16::from_le_bytes([md.tables[off], md.tables[off + 1]]);
            off += 2;
            let minor = u16::from_le_bytes([md.tables[off], md.tables[off + 1]]);
            off += 2;
            let build = u16::from_le_bytes([md.tables[off], md.tables[off + 1]]);
            off += 2;
            let rev = u16::from_le_bytes([md.tables[off], md.tables[off + 1]]);
            off += 2;
            let flags = read_u32_table(&md.tables, off);
            off += 4;
            let public_key = read_idx(&md.tables, off, blob_idx_size);
            off += blob_idx_size;
            let name_idx = read_idx(&md.tables, off, strings_idx_size);
            off += strings_idx_size;
            let culture_idx = read_idx(&md.tables, off, strings_idx_size);
            off += strings_idx_size;
            let hash_value = read_idx(&md.tables, off, blob_idx_size);
            let name = read_strings_string(&md.strings, name_idx);
            out.push_str(&format!(
                "    [{}] Version={}.{}.{}.{} Flags=0x{:08X} Name=\"{}\" PublicKey={} Culture={} Hash={}\n",
                i + 1,
                major,
                minor,
                build,
                rev,
                flags,
                name,
                public_key,
                culture_idx,
                hash_value
            ));
        }
    }

    // 方法体反汇编。
    dump_method_bodies(out, md);
}

/// 反汇编方法体 IL 指令。
fn dump_method_bodies(_out: &mut String, _md: &MetadataRoot) {
    // 预留扩展点：需要从外部传入 PE 数据和 .text 节信息。
}

/// 输出 `#US` 堆中的用户字符串。
pub fn dump_user_strings(md: &MetadataRoot) -> String {
    let mut out = String::new();
    out.push_str("\n=== #US (User Strings) ===\n");
    let us = &md.user_strings;
    if us.is_empty() {
        out.push_str("  (empty)\n");
        return out;
    }
    let mut offset = 1u32; // 跳过初始 0 字节。
    while (offset as usize) < us.len() {
        let s = read_user_string(us, offset);
        // 计算下一个偏移。
        let (len, len_bytes) = read_compressed_uint(us, offset as usize);
        out.push_str(&format!("  [0x{:08X}] {:?}\n", offset, s));
        offset += len_bytes as u32 + len;
    }
    out
}

/// 输出 `#Blob` 堆内容。
pub fn dump_blobs(md: &MetadataRoot) -> String {
    let mut out = String::new();
    out.push_str("\n=== #Blob ===\n");
    let blob = &md.blob;
    if blob.is_empty() {
        out.push_str("  (empty)\n");
        return out;
    }
    let mut offset = 1u32;
    let mut count = 0;
    while (offset as usize) < blob.len() && count < 64 {
        let data = read_blob(blob, offset);
        if data.is_empty() {
            break;
        }
        let (len, len_bytes) = read_compressed_uint(blob, offset as usize);
        out.push_str(&format!("  [0x{:08X}] len={} {:02X?}\n", offset, len, &data[..data.len().min(32)]));
        offset += len_bytes as u32 + len;
        count += 1;
    }
    out
}

/// 将 `MemberRefParent` 结构化解码结果渲染为人类可读描述。
fn format_member_ref_parent(parent: &MemberRefParent) -> String {
    match parent {
        MemberRefParent::TypeDef(row) => format!("TypeDef[{}]", row),
        MemberRefParent::TypeRef(row) => format!("TypeRef[{}]", row),
        MemberRefParent::ModuleRef(row) => format!("ModuleRef[{}]", row),
        MemberRefParent::MethodDef(row) => format!("MethodDef[{}]", row),
        MemberRefParent::TypeSpec(row) => format!("TypeSpec[{}]", row),
        MemberRefParent::Unknown { tag, row } => format!("Unknown(tag={}, row={})", tag, row),
    }
}

/// 将 `MethodSignature` 结构化解码结果渲染为人类可读签名（如 `void(string)`）。
fn format_method_signature(sig: &MethodSignature) -> String {
    let ret = format_element_type(&sig.return_type);
    let params: Vec<String> = sig.param_types.iter().map(format_element_type).collect();
    format!("{}({})", ret, params.join(", "))
}

/// 将 `ElementType` 结构化解码结果渲染为人类可读类型名。
fn format_element_type(ty: &ElementType) -> String {
    match ty {
        ElementType::Void => "void".to_string(),
        ElementType::Bool => "bool".to_string(),
        ElementType::Char => "char".to_string(),
        ElementType::Int8 => "int8".to_string(),
        ElementType::UInt8 => "uint8".to_string(),
        ElementType::Int16 => "int16".to_string(),
        ElementType::UInt16 => "uint16".to_string(),
        ElementType::Int32 => "int32".to_string(),
        ElementType::UInt32 => "uint32".to_string(),
        ElementType::Int64 => "int64".to_string(),
        ElementType::UInt64 => "uint64".to_string(),
        ElementType::Float32 => "float32".to_string(),
        ElementType::Float64 => "float64".to_string(),
        ElementType::String => "string".to_string(),
        ElementType::IntPtr => "IntPtr".to_string(),
        ElementType::UIntPtr => "UIntPtr".to_string(),
        ElementType::Object => "object".to_string(),
        ElementType::SzArray(inner) => format!("{}[]", format_element_type(inner)),
        ElementType::Complex(et) => format!("ElementType_0x{:02X}", et),
        ElementType::Unknown(et) => format!("Unknown_0x{:02X}", et),
    }
}
