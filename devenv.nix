{ pkgs, ... }:

{
  # Packages to install
  packages = with pkgs; [
    redis
    darwin.apple_sdk.frameworks.SystemConfiguration
  ];

  languages.rust = {
    enable = true;
  };

  # Set up environment variables
  env = {
    REDIS_URL = "redis://localhost:6379";  # Add Redis URL
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
  services.redis = {
    enable = true;
    port = 6379;
  };
  # Add any other project-specific configurations here
}