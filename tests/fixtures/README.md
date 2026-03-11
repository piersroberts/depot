# Test Fixtures

These files are used by the SMB test suite (`smb_test.sh`).

## Files

- `test.txt` - Simple text file for content verification
- `subdir/nested.txt` - Nested file for subdirectory access testing
- `sample.bin` - 1KB binary file with deterministic content for integrity testing

## Usage

Configure depot.toml to serve this directory as a share:

```toml
[[shares]]
name = "TestFixtures"
path = "tests/fixtures"
```

Then run the test suite:

```bash
./tests/smb_test.sh
```
