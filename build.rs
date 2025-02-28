fn main() {
    // Generate C header file in target dir
       cbindgen::generate(".")
        .expect("Header Generation failed.")
        .write_to_file("target/cantarasonglib.h");
}