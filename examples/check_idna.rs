use ada_url::Idna;
use serde_json;
fn main() {
    let data = std::fs::read_to_string("tests/wpt/toascii.json").unwrap();
    let v: serde_json::Value = serde_json::from_str(&data).unwrap();
    let arr = v.as_array().unwrap();
    let mut wrong = 0;
    for elem in arr.iter() {
        if elem.is_string() { continue; }
        let o = elem.as_object().unwrap();
        let input = o["input"].as_str().unwrap_or("");
        let expected = o.get("output").unwrap();
        let result = Idna::ascii(input);
        if expected.is_null() {
            if !result.is_empty() { eprintln!("SHOULD_FAIL {:?} -> {:?}", input, result); wrong+=1; }
        } else {
            let exp = expected.as_str().unwrap();
            if result != exp { eprintln!("WRONG {:?}: got={:?} exp={:?}", input, result, exp); wrong+=1; }
        }
    }
    eprintln!("Total wrong: {}", wrong);
}
