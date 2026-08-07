use crate::error::{AppError, Result};
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeReport {
    pub path: String,
    pub architecture: String,
    pub subsystem: String,
    pub entry_point: u32,
    pub image_base: u64,
    pub imports: Vec<String>,
    pub graphics_apis: Vec<String>,
}

#[derive(Debug, Clone)]
struct Section {
    virtual_address: u32,
    virtual_size: u32,
    raw_offset: u32,
    raw_size: u32,
}

pub fn inspect_pe(path: &Path) -> Result<PeReport> {
    if !path.is_file() {
        return Err(AppError::InvalidFile(path.display().to_string()));
    }

    let data = fs::read(path)?;
    let pe_offset = read_u32(&data, 0x3c)? as usize;

    if data.get(0..2) != Some(b"MZ") {
        return Err(AppError::InvalidPe("missing MZ header".into()));
    }

    if data.get(pe_offset..pe_offset + 4) != Some(b"PE\0\0") {
        return Err(AppError::InvalidPe("missing PE signature".into()));
    }

    let coff = pe_offset + 4;
    let machine = read_u16(&data, coff)?;
    let section_count = read_u16(&data, coff + 2)? as usize;
    let optional_size = read_u16(&data, coff + 16)? as usize;
    let optional = coff + 20;
    let magic = read_u16(&data, optional)?;

    let (image_base, data_directory_offset, number_of_rva_offset) = match magic {
        0x20b => (read_u64(&data, optional + 24)?, optional + 112, optional + 108),
        0x10b => (
            read_u32(&data, optional + 28)? as u64,
            optional + 96,
            optional + 92,
        ),
        value => {
            return Err(AppError::InvalidPe(format!(
                "unsupported optional header magic 0x{value:x}"
            )))
        }
    };

    let entry_point = read_u32(&data, optional + 16)?;
    let subsystem = read_u16(&data, optional + 68)?;
    let number_of_rva = read_u32(&data, number_of_rva_offset)?;
    let section_table = optional + optional_size;
    let sections = parse_sections(&data, section_table, section_count)?;

    let imports = if number_of_rva > 1 {
        let import_rva = read_u32(&data, data_directory_offset + 8)?;
        parse_imports(&data, import_rva, &sections)?
    } else {
        Vec::new()
    };

    let graphics_apis = detect_graphics(&imports);

    Ok(PeReport {
        path: path.display().to_string(),
        architecture: architecture_name(machine).to_string(),
        subsystem: subsystem_name(subsystem).to_string(),
        entry_point,
        image_base,
        imports,
        graphics_apis,
    })
}

fn parse_sections(data: &[u8], offset: usize, count: usize) -> Result<Vec<Section>> {
    let mut sections = Vec::with_capacity(count);
    for index in 0..count {
        let base = offset
            .checked_add(index.saturating_mul(40))
            .ok_or_else(|| AppError::InvalidPe("section table overflow".into()))?;
        sections.push(Section {
            virtual_size: read_u32(data, base + 8)?,
            virtual_address: read_u32(data, base + 12)?,
            raw_size: read_u32(data, base + 16)?,
            raw_offset: read_u32(data, base + 20)?,
        });
    }
    Ok(sections)
}

fn parse_imports(data: &[u8], import_rva: u32, sections: &[Section]) -> Result<Vec<String>> {
    if import_rva == 0 {
        return Ok(Vec::new());
    }

    let Some(mut offset) = rva_to_offset(import_rva, sections) else {
        return Err(AppError::InvalidPe("import table RVA is outside sections".into()));
    };

    let mut imports = BTreeSet::new();
    for _ in 0..4096 {
        let original_first_thunk = read_u32(data, offset)?;
        let timestamp = read_u32(data, offset + 4)?;
        let forwarder_chain = read_u32(data, offset + 8)?;
        let name_rva = read_u32(data, offset + 12)?;
        let first_thunk = read_u32(data, offset + 16)?;

        if original_first_thunk == 0
            && timestamp == 0
            && forwarder_chain == 0
            && name_rva == 0
            && first_thunk == 0
        {
            break;
        }

        if name_rva != 0 {
            let name_offset = rva_to_offset(name_rva, sections)
                .ok_or_else(|| AppError::InvalidPe("import name RVA is outside sections".into()))?;
            let name = read_c_string(data, name_offset)?;
            if !name.is_empty() {
                imports.insert(name.to_ascii_lowercase());
            }
        }

        offset = offset
            .checked_add(20)
            .ok_or_else(|| AppError::InvalidPe("import table overflow".into()))?;
    }

    Ok(imports.into_iter().collect())
}

fn detect_graphics(imports: &[String]) -> Vec<String> {
    let names: BTreeSet<&str> = imports.iter().map(String::as_str).collect();
    let mut result = Vec::new();

    if names.contains("d3d12.dll") {
        result.push("Direct3D 12".to_string());
    }
    if names.contains("d3d11.dll") || names.contains("dxgi.dll") {
        result.push("Direct3D 11 / DXGI".to_string());
    }
    if names.contains("d3d10.dll") || names.contains("d3d10_1.dll") {
        result.push("Direct3D 10".to_string());
    }
    if names.contains("d3d9.dll") {
        result.push("Direct3D 9".to_string());
    }
    if names.contains("vulkan-1.dll") {
        result.push("Vulkan".to_string());
    }
    if names.contains("opengl32.dll") {
        result.push("OpenGL".to_string());
    }

    result
}

fn architecture_name(machine: u16) -> &'static str {
    match machine {
        0x014c => "x86",
        0x8664 => "x86_64",
        0xaa64 => "arm64",
        0xa641 => "arm64ec",
        _ => "unknown",
    }
}

fn subsystem_name(subsystem: u16) -> &'static str {
    match subsystem {
        1 => "native",
        2 => "windows-gui",
        3 => "windows-console",
        7 => "posix-console",
        9 => "windows-ce",
        10 => "efi-application",
        11 => "efi-boot-service-driver",
        12 => "efi-runtime-driver",
        13 => "efi-rom",
        14 => "xbox",
        16 => "windows-boot-application",
        _ => "unknown",
    }
}

fn rva_to_offset(rva: u32, sections: &[Section]) -> Option<usize> {
    for section in sections {
        let size = section.virtual_size.max(section.raw_size);
        let end = section.virtual_address.checked_add(size)?;
        if rva >= section.virtual_address && rva < end {
            let relative = rva.checked_sub(section.virtual_address)?;
            let offset = section.raw_offset.checked_add(relative)?;
            return usize::try_from(offset).ok();
        }
    }
    None
}

fn read_c_string(data: &[u8], offset: usize) -> Result<String> {
    let tail = data
        .get(offset..)
        .ok_or_else(|| AppError::InvalidPe("string offset outside file".into()))?;
    let length = tail.iter().position(|byte| *byte == 0).unwrap_or(tail.len());
    let bytes = tail
        .get(..length)
        .ok_or_else(|| AppError::InvalidPe("invalid string range".into()))?;
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or_else(|| AppError::InvalidPe(format!("unexpected EOF at 0x{offset:x}")))?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or_else(|| AppError::InvalidPe(format!("unexpected EOF at 0x{offset:x}")))?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u64(data: &[u8], offset: usize) -> Result<u64> {
    let bytes = data
        .get(offset..offset + 8)
        .ok_or_else(|| AppError::InvalidPe(format!("unexpected EOF at 0x{offset:x}")))?;
    Ok(u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn reads_minimal_pe64() {
        let mut data = vec![0u8; 0x400];
        data[0..2].copy_from_slice(b"MZ");
        data[0x3c..0x40].copy_from_slice(&(0x80u32).to_le_bytes());
        data[0x80..0x84].copy_from_slice(b"PE\0\0");
        let coff = 0x84;
        data[coff..coff + 2].copy_from_slice(&(0x8664u16).to_le_bytes());
        data[coff + 2..coff + 4].copy_from_slice(&(1u16).to_le_bytes());
        data[coff + 16..coff + 18].copy_from_slice(&(0xf0u16).to_le_bytes());
        let optional = coff + 20;
        data[optional..optional + 2].copy_from_slice(&(0x20bu16).to_le_bytes());
        data[optional + 16..optional + 20].copy_from_slice(&(0x1000u32).to_le_bytes());
        data[optional + 24..optional + 32].copy_from_slice(&(0x140000000u64).to_le_bytes());
        data[optional + 68..optional + 70].copy_from_slice(&(2u16).to_le_bytes());
        data[optional + 108..optional + 112].copy_from_slice(&(16u32).to_le_bytes());
        let section = optional + 0xf0;
        data[section + 8..section + 12].copy_from_slice(&(0x200u32).to_le_bytes());
        data[section + 12..section + 16].copy_from_slice(&(0x1000u32).to_le_bytes());
        data[section + 16..section + 20].copy_from_slice(&(0x200u32).to_le_bytes());
        data[section + 20..section + 24].copy_from_slice(&(0x200u32).to_le_bytes());

        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("darwinplay-{nonce}.exe"));
        fs::write(&path, data).unwrap();
        let report = inspect_pe(&path).unwrap();
        fs::remove_file(path).unwrap();

        assert_eq!(report.architecture, "x86_64");
        assert_eq!(report.subsystem, "windows-gui");
        assert_eq!(report.entry_point, 0x1000);
        assert_eq!(report.image_base, 0x140000000);
    }
}
