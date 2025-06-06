{ makeRustPlatform
, rustToolchain
, pkg-config
, openssl
}:
(makeRustPlatform {
  cargo = rustToolchain;
  rustc = rustToolchain;
}).buildRustPackage {
  pname = "aimless-onions";
  version = "git";

  nativeBuildInputs = [
    pkg-config
    openssl
  ];

  PKG_CONFIG_PATH = "${openssl.dev}/lib/pkgconfig";

  src = ./.;

  cargoLock = {
    lockFile = ./Cargo.lock;
    outputHashes = {
      "hohibe-0.1.0" = "sha256-4m1kvGQ7oBrbB7Xwfx+IDqQmHhfxZgZJV6aR2tZAF1w=";
    };
  };
}
