# SNPHost OK Command - Output Format Improvements

## Overview

The `snphost ok` command has been enhanced to support JSON output in addition to the existing text format. This makes it easier to parse and process test results programmatically.

## New Features

### 1. JSON Output Support

A new `--json` flag has been added to output test results in structured JSON format.

**Usage:**
```bash
snphost ok --json
```

### 2. Backward Compatibility

The existing text output format remains unchanged and is the default behavior:

```bash
snphost ok
```

## Output Formats

### Text Output (Default)

The traditional hierarchical text output with color-coded status indicators:

```
[ PASS ] - AMD CPU
[ PASS ]   - Microcode support
[ PASS ]   - Secure Memory Encryption (SME)
[ PASS ]     - SME: Enabled in MSR
[ PASS ]   - Secure Encrypted Virtualization (SEV)
[ PASS ]     - SEV firmware version: 1.55
[ PASS ]     - Encrypted State (SEV-ES)
[ PASS ]       - SEV-ES initialized
[ PASS ]     - SEV initialized: Initialized, no guests running
[ PASS ]     - Secure Nested Paging (SEV-SNP)
...
```

### JSON Output (New)

Structured JSON output with hierarchical test results and summary statistics:

```json
{
  "summary": {
    "total": 26,
    "passed": 24,
    "failed": 0,
    "skipped": 2
  },
  "tests": [
    {
      "name": "AMD CPU",
      "status": "pass",
      "sub_tests": [
        {
          "name": "Microcode support",
          "status": "pass",
          "sub_tests": []
        }
      ]
    },
    {
      "name": "Secure Memory Encryption (SME)",
      "status": "pass",
      "sub_tests": [
        {
          "name": "SME: Enabled in MSR",
          "status": "pass",
          "sub_tests": []
        }
      ]
    }
  ]
}
```

## JSON Output Structure

### Top Level

- `summary`: Object containing aggregate statistics
  - `total`: Total number of tests executed
  - `passed`: Number of tests that passed
  - `failed`: Number of tests that failed
  - `skipped`: Number of tests that were skipped
- `tests`: Array of test result objects

### Test Result Object

Each test result object contains:
- `name`: String describing the test
- `status`: Test status (`"pass"`, `"fail"`, or `"skip"`)
- `message`: (Optional) Additional information or error message
- `sub_tests`: Array of nested test result objects

## Use Cases

### Monitoring and Alerting

Parse JSON output to check for failures:

```bash
#!/bin/bash
result=$(snphost ok --json)
failed=$(echo "$result" | jq '.summary.failed')

if [ "$failed" -gt 0 ]; then
    echo "WARNING: $failed tests failed"
    exit 1
fi
```

### CI/CD Integration

Store test results for trending and analysis:

```bash
snphost ok --json > test-results-$(date +%Y%m%d).json
```

### Dashboard Integration

Use jq to extract specific test results:

```bash
# Check SEV-SNP support
snphost ok --json | jq '.tests[] | select(.name == "Secure Nested Paging (SEV-SNP)") | .status'

# Get all failed tests
snphost ok --json | jq '.tests[] | select(.status == "fail")'
```

### Automated Reporting

Generate reports from JSON output:

```python
import json
import subprocess

result = subprocess.run(['snphost', 'ok', '--json'], capture_output=True, text=True)
data = json.loads(result.stdout)

print(f"Test Summary:")
print(f"  Total: {data['summary']['total']}")
print(f"  Passed: {data['summary']['passed']}")
print(f"  Failed: {data['summary']['failed']}")
print(f"  Skipped: {data['summary']['skipped']}")
```

## Compatibility

- The `--quiet` flag behavior with JSON mode: When using `--json`, the JSON output is always printed regardless of the `--quiet` flag, as it's the primary output format. The `--quiet` flag primarily affects text-based output and error messages
- Exit codes remain unchanged (0 for success, non-zero for failures)
- The JSON flag can be combined with other global flags

## Examples

**Standard text output:**
```bash
snphost ok
```

**JSON output:**
```bash
snphost ok --json
```

**Suppress error messages (JSON output still shown):**
```bash
snphost ok --json --quiet
```

**Pretty-printed JSON for human reading:**
```bash
snphost ok --json | jq .
```

**Check if all tests passed:**
```bash
snphost ok --json | jq -e '.summary.failed == 0'
```

**Extract test names and statuses:**
```bash
snphost ok --json | jq '.tests[] | {name, status}'
```

## Implementation Details

The implementation adds:
1. Serde serialization support to test result structures
2. Hierarchical test result collection for JSON output
3. Summary statistics calculation
4. Clean separation between text and JSON output modes

All changes maintain backward compatibility with existing workflows and scripts.
