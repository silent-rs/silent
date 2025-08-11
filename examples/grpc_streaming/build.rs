fn main() {
    tonic_prost_build::configure()
        .compile_protos(&["proto/echo.proto"], &["/proto"])
        .unwrap();
}
