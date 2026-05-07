use std::collections::HashMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

#[derive(Debug)]
pub struct Object {
    pub class: String,
    pub fields: HashMap<String, String>,
}

#[derive(Debug)]
pub struct Db {
    objects: HashMap<String, Object>,
}

impl Db {
    pub fn parse(xml: &str) -> Result<Db, Box<dyn std::error::Error>> {
        let mut reader = Reader::from_str(xml);
        let mut buf = Vec::new();
        let mut db_objs: HashMap<String, Object> = HashMap::new();

        // As XAPI DB is flat (no nested table) we don't need to use a stack to
        // parse the database. Storing the current table is enough
        let mut current_table: Option<String> = None;

        loop {
            match reader.read_event_into(&mut buf)? {
                Event::Eof => break,
                Event::Start(e) if e.local_name().as_ref() == b"table" => {
                    if current_table.is_some() {
                        return Err(format!("nested table not expected {:?}", current_table).into());
                    }
                    current_table = Some(parse_table_name(&e)?)
                }
                Event::Start(_) => (), // We are only interesting by table, ignore all others
                Event::End(e) if e.local_name().as_ref() == b"table" => current_table = None,
                Event::End(_) => (),
                Event::Empty(e) if e.local_name().as_ref() == b"row" => {
                    let class = current_table
                        .as_ref()
                        .ok_or("Found a row but table is not set")?
                        .clone();
                    let (opaque_ref, fields) = parse_row(&e)?;
                    db_objs.insert(opaque_ref, Object { class, fields });
                }
                Event::Empty(_) => (), // We are only interesting by row, ignore all others
                Event::Text(e) => {
                    let bytes = e.as_ref(); // &[u8]
                    if bytes.iter().any(|b| !b.is_ascii_whitespace()) {
                        return Err(format!("Text {:?}", e).into());
                    }
                    // else: ignore whitespace
                }
                Event::Comment(_) | Event::Decl(_) => (),
                Event::CData(e) => return Err(format!("CData {:?}", e).into()),
                Event::PI(e) => return Err(format!("PI {:?}", e).into()),
                Event::DocType(e) => return Err(format!("DocType {:?}", e).into()),
                Event::GeneralRef(e) => return Err(format!("GeneralRef {:?}", e).into()),
            }

            buf.clear();
        }

        Ok(Db { objects: db_objs })
    }

    pub fn get(&self, opaque_ref: &str) -> Option<&Object> {
        self.objects.get(opaque_ref)
    }
}

fn parse_table_name(e: &BytesStart<'_>) -> Result<String, Box<dyn std::error::Error>> {
    for attr in e.attributes() {
        let attr = attr?;
        if attr.key.as_ref() == b"name" {
            return Ok(attr.unescape_value()?.into_owned());
        }
    }

    Err("table has no name attribute".into())
}

fn parse_row(
    e: &BytesStart<'_>,
) -> Result<(String, HashMap<String, String>), Box<dyn std::error::Error>> {
    let mut fields: HashMap<String, String> = HashMap::new();
    let mut opaque_ref: Option<String> = None;

    // We iterate over row to create the object, find the opaque ref
    // and add it in the DB.
    for attr in e.attributes() {
        let attr = attr?;
        match attr.key.as_ref() {
            b"_ref" => {
                if let Some(opaque) = &opaque_ref {
                    return Err(format!("already found the opaque ref {}", opaque).into());
                }
                opaque_ref = Some(attr.unescape_value()?.into_owned());
            }
            b"ref" => {} // it seems to be an alias with _ref so can be ignored.
            a => {
                let key = std::str::from_utf8(a)?.to_string();
                let value = attr.unescape_value()?.into_owned();
                fields.insert(key, value);
            }
        }
    }

    let opaque_ref = opaque_ref.ok_or("row has no opaque ref")?;
    Ok((opaque_ref, fields))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY_DB: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <manifest><pair key="schema_major_vsn" value="5"/></manifest>
  <table name="VM">
    <row ref="OpaqueRef:vm-1" _ref="OpaqueRef:vm-1" uuid="uuid-vm-1" name_label="alice" power_state="Running"/>
    <row ref="OpaqueRef:vm-2" _ref="OpaqueRef:vm-2" uuid="uuid-vm-2" name_label="bob" power_state="Halted"/>
  </table>
  <table name="host">
    <row ref="OpaqueRef:host-1" _ref="OpaqueRef:host-1" uuid="uuid-host-1" name_label="cloud-edge-1"/>
  </table>
  <table name="Bond"/>
</database>"#;

    #[test]
    fn parse_three_objects() {
        let db = Db::parse(TINY_DB).unwrap();
        let vm = db.get("OpaqueRef:vm-1").expect("vm-1 should exist");
        assert_eq!(vm.class, "VM");
        assert_eq!(
            vm.fields.get("name_label").map(String::as_str),
            Some("alice")
        );
        assert_eq!(
            vm.fields.get("power_state").map(String::as_str),
            Some("Running")
        );
    }

    #[test]
    fn missing_ref_is_an_error() {
        let xml = r#"<database><table name="VM"><row uuid="x"/></table></database>"#;
        assert!(Db::parse(xml).is_err());
    }

    #[test]
    fn rejects_non_whitespace_text() {
        let xml = r#"<database><table name="VM"><row _ref="OpaqueRef:1"/>oops</table></database>"#;
        assert!(Db::parse(xml).is_err());
    }
}
