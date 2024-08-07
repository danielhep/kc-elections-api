{ pkgs ? import <nixpkgs> {} }:

let
  rustPlatform = pkgs.rustPlatform;
in
pkgs.dockerTools.buildLayeredImage {
  name = "kcelectionapi";
  tag = "latest";
  contents = [
    (rustPlatform.buildRustPackage {
      pname = "kcelectionapi";
      version = "0.1.0";
      src = ./.;
      cargoLock = {
        lockFile = ./Cargo.lock;
      };
      nativeBuildInputs = [ pkgs.pkg-config ];
      buildInputs = [ pkgs.openssl pkgs.libiconv ];
    })
    pkgs.cacert
  ];

  config = {
    Cmd = [ "/bin/kcelectionapi" ];
    ExposedPorts = {
      "8080/tcp" = {};
    };
    Env = [
      "SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
      "REDIS_URL=redis://redis:6379"
    ];
  };
}