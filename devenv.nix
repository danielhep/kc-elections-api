{ pkgs, ... }:

{
  # Packages to install
  packages = with pkgs; [
    redis
  ] ++ lib.optionals pkgs.stdenv.isDarwin [
    pkgs.darwin.apple_sdk.frameworks.CoreFoundation
    pkgs.darwin.apple_sdk.frameworks.Security
    pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
    pkgs.darwin.apple_sdk.frameworks.Cocoa
  ];

  languages.rust = {
    enable = true;
  };

  # Set up environment variables
  env = {
    # REDIS_URL = "redis://localhost:6379";  # Add Redis URL
    DATABASE_URL = "postgresql://postgres@localhost/kcelections";  # Updated for PostgreSQL
    CSV_URL = "https://aqua.kingcounty.gov/elections/2024/aug-primary/webresults.csv";
    GOATCOUNTER_URL = "https://danielhep.goatcounter.com/count";
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
  # services.redis = {
  #   enable = true;
  #   port = 6379;
  # };
 # PostgreSQL service configuration
  services.postgres = {
    enable = true;
    package = pkgs.postgresql_14;  # You can change this to the version you prefer
    initialDatabases = [{ name = "kcelections"; }];
    initialScript = ''
      CREATE USER postgres SUPERUSER;
      CREATE DATABASE kcelections WITH OWNER postgres;
    '';
    listen_addresses = "127.0.0.1";
  };
  # Add any other project-specific configurations here
}