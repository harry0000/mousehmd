fn main() {
    println!("cargo:rerun-if-changed=cpp/shim.cpp");
    println!("cargo:rerun-if-changed=cpp/openvr_driver.h");
    println!("cargo:rerun-if-env-changed=MOUSEHMD_SKIP_CPP");
    // `cargo check` on a machine without a Windows C++ compiler.
    if std::env::var_os("MOUSEHMD_SKIP_CPP").is_some() {
        return;
    }
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .include("cpp")
        .file("cpp/shim.cpp")
        .warnings(false);
    // MSVC only implements C++ exception semantics (unwinding into catch blocks)
    // with /EHsc, and cc does not pass it. The shim catches std::system_error.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        build.flag("/EHsc");
    }
    build.compile("mousehmd_shim");
}
