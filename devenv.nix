{ pkgs, ... }:

{
  # Packages to install
  packages = with pkgs; [
    rustc
    cargo
    rust-analyzer
    clippy
    rustfmt
    pkg-config
    openssl
    libiconv  # Add this line
    redis
  ];

  # Set up environment variables
  env = {
    RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
    RUSTFLAGS = "-L ${pkgs.libiconv}/lib";  # Add this line
    REDIS_URL = "redis://localhost:6379";  # Add Redis URL
  };

  # Shell configuration
  enterShell = ''
    echo "Rust development environment loaded!"
    echo "Rust version: $(rustc --version)"
    echo "Cargo version: $(cargo --version)"
    echo "Redis URL: $REDIS_URL"
  '';

  # Project-specific configurations
  dotenv.enable = true;
  dotenv.filename = ".env";

  # Pre-commit hooks
  pre-commit.hooks = {
    clippy.enable = true;
    rustfmt.enable = true;
  };

  # Redis service configuration
  services.redis = {
    enable = true;
    port = 6379;
  };
  # Add any other project-specific configurations here
}