use std::ffi::{CStr, CString, c_char};
use std::ptr;

use cantara_songlib::{
    create_abc_from_file_c, create_lilypond_from_file_c, create_presentation_from_file_c,
    create_song_yml_from_file_c, create_text_from_file_c, free_c_string, get_song_from_file_as_json_c,
};

fn read_and_free(ptr: *const c_char) -> String {
    assert!(!ptr.is_null(), "ffi function returned null");
    let text = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };
    free_c_string(ptr as *mut c_char);
    text
}

#[test]
fn test_get_song_from_file_as_json_c() {
    let file = CString::new("tests/data/Amazing Grace.song.yml").unwrap();
    let output = get_song_from_file_as_json_c(file.as_ptr());
    let json = read_and_free(output);
    assert!(json.contains("\"title\": \"Amazing Grace\""), "{}", json);
}

#[test]
fn test_create_presentation_from_file_c() {
    let file = CString::new("tests/data/Amazing Grace.song.yml").unwrap();
    let meta = CString::new("{{title}}").unwrap();

    let output = create_presentation_from_file_c(file.as_ptr(), 1, 1, 7, meta.as_ptr(), 1, 0);
    let slides_json = read_and_free(output);
    assert!(serde_json::from_str::<serde_json::Value>(&slides_json).is_ok(), "{}", slides_json);
}

#[test]
fn test_create_text_from_file_c() {
    let file = CString::new("tests/data/Amazing Grace.song.yml").unwrap();
    let format = CString::new("plain").unwrap();

    let output = create_text_from_file_c(
        file.as_ptr(),
        format.as_ptr(),
        ptr::null(),
        ptr::null(),
        ptr::null(),
    );
    let text = read_and_free(output);
    assert!(text.starts_with("Amazing Grace"), "{}", text);
}

#[test]
fn test_create_lilypond_from_file_c() {
    let file = CString::new("tests/data/Amazing Grace.song.yml").unwrap();
    let paper_size = CString::new("a4").unwrap();
    let indent = CString::new("#0").unwrap();

    let output = create_lilypond_from_file_c(file.as_ptr(), paper_size.as_ptr(), indent.as_ptr());
    let lilypond = read_and_free(output);
    assert!(lilypond.contains("\\score"), "{}", lilypond);
}

#[test]
fn test_create_abc_from_file_c() {
    let file = CString::new("tests/data/Amazing Grace.song.yml").unwrap();
    let unit = CString::new("1/4").unwrap();

    let output = create_abc_from_file_c(file.as_ptr(), unit.as_ptr(), 1, 1);
    let abc = read_and_free(output);
    assert!(abc.contains("\nK:"), "{}", abc);
}

#[test]
fn test_create_song_yml_from_file_c() {
    let file = CString::new("tests/data/Amazing Grace.song").unwrap();
    let output = create_song_yml_from_file_c(file.as_ptr());
    let song_yml = read_and_free(output);
    assert!(song_yml.contains("title: Amazing Grace"), "{}", song_yml);
}
