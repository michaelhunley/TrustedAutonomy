  # Manual version bump and release for alpha testing
  git tag v0.1.0-alpha
  git push origin v0.1.0-alpha
  gh release create v0.1.0-alpha --title "v0.1.0-alpha" --generate-notes --prerelease