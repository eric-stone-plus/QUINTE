# Prebuilt GitHub Releases are no longer published.
# Build from source:
#   git clone https://github.com/eric-stone-plus/QUINTE.git
#   cd QUINTE; cargo build --release
#   copy target\release\quinte.exe $env:LOCALAPPDATA\Programs\quinte\bin\
Write-Error "quinte: prebuilt installer retired; build from source (see README.md)"
exit 1
