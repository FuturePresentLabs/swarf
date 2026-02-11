# Contributing to swarf

Thank you for your interest in contributing to swarf! We welcome contributions from the community.

## Contributor License Agreement (CLA)

**IMPORTANT:** Before we can accept any contributions, you must agree to our Contributor License Agreement (CLA).

By contributing to swarf, you agree that:
1. You have read and agree to the [CLA](CLA.md)
2. You assign copyright of your contributions to **Future Present Labs LLC**
3. You grant us a perpetual license to use your contributions

### How to Sign the CLA

When submitting your first Pull Request, include this statement in the PR description:

```
I, [Your Full Legal Name], agree to the terms of the Future Present Labs LLC 
Contributor License Agreement v1.0 as found in CLA.md.
```

Alternatively, email a signed CLA to: **cla@futurepresentlabs.com**

## Development Setup

```bash
# Clone the repository
git clone https://github.com/FuturePresentLabs/swarf.git
cd swarf

# Build
cargo build

# Run tests
cargo test

# Check formatting
cargo fmt -- --check

# Run clippy
cargo clippy
```

## Submitting Changes

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Add tests if applicable
5. Ensure all tests pass (`cargo test`)
6. Sign the CLA in your PR description
7. Submit a Pull Request

## Code Style

- Follow Rust naming conventions
- Run `cargo fmt` before committing
- Keep functions focused and small
- Add documentation comments for public APIs

## Questions?

Open an issue or email: contact@futurepresentlabs.com

Thank you for contributing!
