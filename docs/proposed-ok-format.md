# Proposed `snphost ok` Output Formats

This document proposes several alternative output formats for the `snphost ok` command.

## Current Format (for reference)

<details open>
<summary><b>Success case (click to collapse)</b></summary>

```
[ PASS ] - AMD CPU
[ PASS ]   - EPYC processor detected
[ PASS ]   - Microcode support
[ PASS ]   - Secure Memory Encryption (SME)
[ PASS ]     - SME supported
[ PASS ]     - SME enabled in MSR
[ PASS ]   - Secure Encrypted Virtualization (SEV)
[ PASS ]     - SEV supported
[ PASS ]     - SEV firmware version: 1.55
[ PASS ]     - SEV-ES supported
[ PASS ]       - SEV-ES initialized
[ PASS ]     - SEV initialized: Initialized, no guests running
[ PASS ]     - Secure Nested Paging (SEV-SNP)
[ PASS ]       - SEV-SNP supported
[ PASS ]       - VM Permission Levels
[ PASS ]         - Number of VMPLs: 4
[ PASS ]       - SNP enabled in MSR
[ PASS ]       - SNP initialized
[ PASS ]         - RMP table base address: 0x18278900000 - 0x183fc9fffff
[ PASS ]         - RMP table initialized
[ PASS ]         - Alias check: Completed since last system update, no aliasing addresses
[ PASS ]     - Physical address bit reduction: 6
[ PASS ]     - C-bit location: 51
[ PASS ]     - Number of encrypted guests supported simultaneously: 1006
[ PASS ]     - Minimum ASID value for SEV-ES-enabled, SEV-only disabled guest: 500
[ PASS ]     - /dev/sev readable
[ PASS ]     - /dev/sev writable
[ PASS ]   - Page flush MSR: Enabled
[ PASS ] - KVM supported: API version: 12
[ PASS ]   - SEV enabled in KVM
[ PASS ]   - SEV-ES enabled in KVM
[ PASS ]   - SEV-SNP enabled in KVM
[ PASS ] - Memlock resource limit: Soft: 202050342912 | Hard: 202050342912
[ PASS ] - Comparing TCB values: TCB versions match

 Platform TCB version: TCB Version:
  Microcode:   25
  SNP:         27
  TEE:         0
  Boot Loader: 10
  FMC:         None
 Reported TCB version: TCB Version:
  Microcode:   25
  SNP:         27
  TEE:         0
  Boot Loader: 10
  FMC:         None
```

</details>

<details>
<summary><b>Error case (click to expand)</b></summary>

```
[ PASS ] - AMD CPU
[ PASS ]   - Microcode support
[ PASS ]   - Secure Memory Encryption (SME)
[ FAIL ]     - SME: MSR read failed: Error Reading MSR
[ PASS ]   - Secure Encrypted Virtualization (SEV)
[ FAIL ]     - SEV firmware version: Failed to get SEV Platform Status unable to open /dev/sev
[ PASS ]     - SEV-ES support
[ FAIL ]       - SEV-ES initialized: Failed to get SEV Platform Status unable to open /dev/sev
[ FAIL ]     - SEV initialized: Failed to get SEV Platform Status unable to open /dev/sev
[ PASS ]     - Secure Nested Paging (SEV-SNP)
[ PASS ]       - VM Permission Levels
[ PASS ]         - Number of VMPLs: 4
[ FAIL ]       - SNP: MSR read failed: Error Reading MSR
[ FAIL ]       - SNP initialized: Failed to get SNP Platform status unable to open /dev/sev
[ SKIP ]         - Read RMP tables
[ SKIP ]         - RMP table initialized
[ SKIP ]         - Alias check
...
ERROR: One or more tests in snphost ok reported a failure
Error: One or more tests in snphost ok reported a failure
```

</details>

**Issues:**
- Doesn't indicate root cause (permissions, missing module, BIOS setting)
- No actionable guidance

---

## Proposal 1: Minimal - Software Version Checks Added at Bottom

Overly conservative to avoid script breakage, add any clarifying information in parenthesis.

<details open>
<summary><b>Success case (click to collapse)</b></summary>

```
$ sudo snphost ok
[ PASS ] - AMD CPU
[ PASS ]   - Microcode support 									(CPU is EPYC)
[ PASS ]   - Secure Memory Encryption (SME) 					(CPU Support)
[ PASS ]     - SME: Enabled in MSR								(BIOS Config)
[ PASS ]   - Secure Encrypted Virtualization (SEV) 				(CPU Support)
[ PASS ]     - SEV firmware version: 1.55
[ PASS ]     - Encrypted State (SEV-ES) 						(CPU Support)
[ PASS ]       - SEV-ES initialized 							(FW Ready)
[ PASS ]     - SEV initialized: Initialized, no guests running 	(FW Ready)
[ PASS ]     - Secure Nested Paging (SEV-SNP)					(CPU Support)
[ PASS ]       - VM Permission Levels
[ PASS ]         - Number of VMPLs: 4
[ PASS ]       - SNP: Enabled in MSR							(BIOS Config)
[ PASS ]       - SNP initialized								(FW Ready)
[ PASS ]         - RMP table addresses: 0x18278900000 - 0x183fc9fffff
[ PASS ]         - RMP table initialized
[ PASS ]         - Alias check: Completed since last system update, no aliasing addresses
[ PASS ]     - Physical address bit reduction: 6
[ PASS ]     - C-bit location: 51
[ PASS ]     - Number of encrypted guests supported simultaneously: 1006
[ PASS ]     - Minimum ASID value for SEV-enabled, SEV-ES disabled guest: 500
[ PASS ]     - /dev/sev readable
[ PASS ]     - /dev/sev writable
[ PASS ]   - Page flush MSR: DISABLED
[ PASS ] - KVM supported: API version: 12
[ PASS ]   - SEV enabled in KVM
[ PASS ]   - SEV-ES enabled in KVM
[ PASS ]   - SEV-SNP enabled in KVM
[ PASS ] - Memlock resource limit: Soft: 202050342912 | Hard: 202050342912
[ PASS ] - Comparing TCB values: TCB versions match

Installed Components:
[ PASS ] - QEMU [/usr/bin/qemu-system-x86_64]: 1:9.2.1+ds-1ubuntu5.2 (min: 6.0)
[ WARN ] - libvirt: Not installed (min: 4.5)
[ PASS ] - OVMF [/usr/share/ovmf/OVMF.amdsev.fd]: 2025.02-3ubuntu2.2
[ PASS ] - Kernel: 6.14.0-37-generic (min: 6.11)
[ PASS ] - SEV Firmware: 1.55 (min: 1.51)
```

</details>

<details>
<summary><b>Error case (click to expand)</b></summary>

```
[ PASS ] - AMD CPU
[ PASS ]   - Microcode support
[ PASS ]   - Secure Memory Encryption (SME)
[ FAIL ]     - SME enabled: MSR read failed (run with sudo + modprobe msr)
[ PASS ]   - Secure Encrypted Virtualization (SEV)
[ FAIL ]     - SEV firmware version: Permission denied (run with sudo)
[ PASS ]     - SEV-ES support
[ FAIL ]       - SEV-ES initialized: Permission denied (run with sudo)
[ FAIL ]     - SEV initialized: Permission denied (run with sudo)
[ PASS ]     - Secure Nested Paging (SEV-SNP)
[ PASS ]       - VM Permission Levels
[ PASS ]         - Number of VMPLs: 4
[ FAIL ]       - SNP enabled: MSR read failed (run with sudo + modprobe msr)
[ FAIL ]       - SNP initialized: Permission denied (run with sudo)
[ SKIP ]         - Read RMP tables
[ SKIP ]         - RMP table initialized
[ SKIP ]         - Alias check
...
Installed Components:
[ PASS ] - QEMU [/usr/bin/qemu-system-x86_64]: 1:9.2.1+ds-1ubuntu5.2 (min: 6.0)
[ WARN ] - libvirt: Not installed (min: 4.5)
[ PASS ] - OVMF [/usr/share/ovmf/OVMF.amdsev.fd]: 2025.02-3ubuntu2.2
[ PASS ] - Kernel: 6.14.0-37-generic (min: 6.11)
[ FAIL ] - SEV Firmware version: Permission denied (run with sudo)
...
ERROR: Multiple tests failed due to permissions. Run with: sudo snphost ok
```

</details>

**Pros:**
- Minimal change to existing format
- Preserves all existing test output
- Adds immediate actionable hints

**Cons:**
- Still noisy when many tests fail for same reason
- Doesn't prevent repeating same hint multiple times

## Proposal 1a: Minimal - Clarify Test Names

**Change:** Same as Proposal 1, but add clarifying labels (CPU/BIOS/Ready/KVM) to test names.

<details open>
<summary><b>Success case (click to collapse)</b></summary>

```
[ PASS ] - AMD CPU
[ PASS ]   - EPYC processor detected (Genoa)
[ PASS ]   - Secure Memory Encryption (SME)
[ PASS ]     - SME support (CPU)
[ PASS ]     - SME enabled (BIOS)
[ PASS ]   - Secure Encrypted Virtualization (SEV)
[ PASS ]     - SEV support (CPU)
[ PASS ]     - SEV firmware version: 1.55
[ PASS ]     - SEV-ES support (CPU)
[ PASS ]       - SEV-ES initialized (Ready)
[ PASS ]     - SEV initialized (Ready): Initialized, no guests running
[ PASS ]     - Secure Nested Paging (SEV-SNP)
[ PASS ]       - SEV-SNP support (CPU)
[ PASS ]       - VM Permission Levels
[ PASS ]         - Number of VMPLs: 4
[ PASS ]       - SNP enabled (BIOS)
[ PASS ]       - SNP initialized (Ready)
[ PASS ]         - RMP table base address: 0x18278900000 - 0x183fc9fffff
[ PASS ]         - RMP table initialized
[ PASS ]         - Alias check: Completed since last system update, no aliasing addresses
...
[ PASS ] - Installed software versions
[ PASS ]   - QEMU version: 1:9.2.1+ds-1ubuntu5.2 (min: 6.0)
[ PASS ]   - OVMF version: 2025.02-3ubuntu2.2
[ PASS ]   - Kernel version: 6.14.0-37-generic (min: 6.11)
[ WARN ]   - libvirt: Not installed (min: 4.5)
[ PASS ]   - SEV Firmware: 1.55 (min: 1.51)
```

</details>

<details>
<summary><b>Error case (click to expand)</b></summary>

```
[ PASS ] - AMD CPU
[ PASS ]   - EPYC processor detected (Genoa)
[ PASS ]   - Secure Memory Encryption (SME)
[ PASS ]     - SME support (CPU)
[ FAIL ]     - SME enabled (BIOS): MSR read failed (run with sudo + modprobe msr)
[ PASS ]   - Secure Encrypted Virtualization (SEV)
[ PASS ]     - SEV support (CPU)
[ FAIL ]     - SEV firmware version: Permission denied (run with sudo)
[ PASS ]     - SEV-ES support (CPU)
[ FAIL ]       - SEV-ES initialized (Ready): Permission denied (run with sudo)
[ FAIL ]     - SEV initialized (Ready): Permission denied (run with sudo)
[ PASS ]     - Secure Nested Paging (SEV-SNP)
[ PASS ]       - SEV-SNP support (CPU)
[ PASS ]       - VM Permission Levels
[ PASS ]         - Number of VMPLs: 4
[ FAIL ]       - SNP enabled (BIOS): MSR read failed (run with sudo + modprobe msr)
[ FAIL ]       - SNP initialized (Ready): Permission denied (run with sudo)
[ SKIP ]         - Read RMP tables
[ SKIP ]         - RMP table initialized
[ SKIP ]         - Alias check
...
[ PASS ] - Installed software versions
[ PASS ]   - QEMU version: 1:9.2.1+ds-1ubuntu5.2 (min: 6.0)
[ PASS ]   - OVMF version: 2025.02-3ubuntu2.2
[ PASS ]   - Kernel version: 6.14.0-37-generic (min: 6.11)
[ WARN ]   - libvirt: Not installed (min: 4.5)
[ FAIL ]   - SEV Firmware version: Permission denied (run with sudo)
...
ERROR: Multiple tests failed due to permissions. Run with: sudo snphost ok
```

</details>

**Pros:**
- Minimal change to existing format
- Preserves all existing test output
- Adds immediate actionable hints
- Clarifies what type of check each test is (CPU capability, BIOS setting, Ready state, KVM config)

**Cons:**
- Still noisy when many tests fail for same reason
- Doesn't prevent repeating same hint multiple times
- Test names slightly longer

---

## Proposal 2: Moderate - Category Summary

**Change:** Group tests by category, add summary section at end.

<details open>
<summary><b>Success case (click to collapse)</b></summary>

```
CPU & Platform:
  [ PASS ] AMD Processor
  [ PASS ] AMD EPYC Processor (Genoa)
  [ PASS ] Physical address bits: 6
  [ PASS ] C-bit position: 51
  [ PASS ] Firmware version: 1.55
  [ PASS ] /dev/sev readable
  [ PASS ] /dev/sev writable
  [ PASS ] KVM support (API version: 12)
  [ PASS ] Memlock limit: Soft 202050342912 | Hard 202050342912

SME:
  [ PASS ] SME support (CPU)
  [ PASS ] SME enabled (BIOS)

SEV:
  [ PASS ] SEV support (CPU)
  [ PASS ] Max encrypted guests: 1006
  [ PASS ] Min ASID for SEV-ES/SNP: 500
  [ PASS ] Page flush MSR: Enabled
  [ PASS ] SEV initialized (Ready)
  [ PASS ] SEV enabled in KVM

SEV-ES:
  [ PASS ] SEV-ES support (CPU)
  [ PASS ] SEV-ES initialized (Ready)
  [ PASS ] SEV-ES enabled in KVM

SEV-SNP:
  [ PASS ] SEV-SNP support (CPU)
  [ PASS ] VMPL support: 4 VMPLs
  [ PASS ] SNP enabled (BIOS)
  [ PASS ] RMP addresses (BIOS): 0x18278900000 - 0x183fc9fffff
  [ PASS ] SNP initialized (Ready)
  [ PASS ] RMP initialized (Ready)
  [ PASS ] SEV-SNP enabled in KVM
  [ PASS ] Memory alias check: Completed since last system update, no aliasing addresses
  [ PASS ] TCB version comparison: Platform TCB matches reported TCB

Installed Components:
  [ PASS ] QEMU [/usr/bin/qemu-system-x86_64]: 1:9.2.1+ds-1ubuntu5.2 (min: 6.0)
  [ WARN ] libvirt: Not installed (min: 4.5)
  [ PASS ] OVMF [/usr/share/ovmf/OVMF.amdsev.fd]: 2025.02-3ubuntu2.2
  [ PASS ] Kernel: 6.14.0-37-generic (min: 6.11)
  [ PASS ] SEV Firmware: 1.55 (min: 1.51)

─────────────────────────────────────────────
SUMMARY: 27 passed, 0 failed, 1 skipped (28 total)

OK
```

</details>

<details>
<summary><b>Error case (click to expand)</b></summary>

```
CPU & Platform:
  [ PASS ] AMD Processor
  [ PASS ] AMD EPYC Processor (Genoa)
  [ PASS ] Physical address bits: 6
  [ PASS ] C-bit position: 51
  [ FAIL ] Firmware version: Permission denied (need sudo)
  [ FAIL ] /dev/sev readable: Permission denied (need sudo)
  [ FAIL ] /dev/sev writable: Permission denied (need sudo)
  [ FAIL ] KVM support: Permission denied (need sudo)
  [ PASS ] Memlock limit: Soft 202050342912 | Hard 202050342912

SME:
  [ PASS ] SME support (CPU)
  [ FAIL ] SME enabled (BIOS): MSR read failed (run: sudo modprobe msr)

SEV:
  [ PASS ] SEV support (CPU)
  [ PASS ] Max encrypted guests: 1006
  [ PASS ] Min ASID for SEV-ES/SNP: 500
  [ PASS ] Page flush MSR: Enabled
  [ FAIL ] SEV initialized (Ready): Permission denied (need sudo)
  [ SKIP ] SEV enabled in KVM

SEV-ES:
  [ PASS ] SEV-ES support (CPU)
  [ FAIL ] SEV-ES initialized (Ready): Permission denied (need sudo)
  [ SKIP ] SEV-ES enabled in KVM

SEV-SNP:
  [ PASS ] SEV-SNP support (CPU)
  [ PASS ] VMPL support: 4 VMPLs
  [ FAIL ] SNP enabled (BIOS): MSR read failed (run: sudo modprobe msr)
  [ FAIL ] RMP addresses (BIOS): MSR read failed (run: sudo modprobe msr)
  [ FAIL ] SNP initialized (Ready): Permission denied (need sudo)
  [ SKIP ] RMP initialized
  [ SKIP ] SEV-SNP enabled in KVM
  [ SKIP ] Memory alias check
  [ SKIP ] TCB version comparison

Installed Components:
  [ PASS ] QEMU [/usr/bin/qemu-system-x86_64]: 1:9.2.1+ds-1ubuntu5.2 (min: 6.0)
  [ WARN ] libvirt: Not installed (min: 4.5)
  [ PASS ] OVMF [/usr/share/ovmf/OVMF.amdsev.fd]: 2025.02-3ubuntu2.2
  [ PASS ] Kernel: 6.14.0-37-generic (min: 6.11)
  [ FAIL ] SEV Firmware: Permission denied (min: 1.51)

─────────────────────────────────────────────
SUMMARY: 13 passed, 9 failed, 6 skipped

DETECTED ISSUES:
  • Permission denied accessing /dev/sev
    → Run with sudo: sudo snphost ok

  • MSR read failures
    → Load MSR module: sudo modprobe msr
```

</details>

**Pros:**
- Groups related tests together
- Summary clearly shows what category of problems exist
- Reduces duplicate error messages
- Easy to see at a glance what's wrong

**Cons:**
- Changes test grouping (might break scripts parsing output)
- Loses some hierarchy detail

---

## Proposal 2a: Moderate - Organized by Test Category

**Change:** Group tests by test category (columns from reference table) instead of technology.

<details open>
<summary><b>Success case (click to collapse)</b></summary>

```
CPU Support:
  [ PASS ] AMD Processor
  [ PASS ] AMD EPYC Processor (Genoa)
  [ PASS ] SME support
  [ PASS ] SEV support
  [ PASS ] SEV-ES support
  [ PASS ] SEV-SNP support

CPU Info:
  [ PASS ] Physical address bits: 6
  [ PASS ] C-bit position: 51
  [ PASS ] Max encrypted guests: 1006
  [ PASS ] Min ASID for SEV-ES/SNP: 500
  [ PASS ] Page flush MSR: Enabled
  [ PASS ] VMPL support: 4 VMPLs

BIOS Configured:
  [ PASS ] Firmware version: 1.55
  [ PASS ] SME enabled
  [ PASS ] SNP enabled
  [ PASS ] RMP addresses: 0x18278900000 - 0x183fc9fffff
  [ PASS ] ASID limits configured

Platform Initialized:
  [ PASS ] /dev/sev readable
  [ PASS ] /dev/sev writable
  [ PASS ] SEV initialized
  [ PASS ] SEV-ES initialized
  [ PASS ] SNP initialized
  [ PASS ] RMP initialized

KVM Config:
  [ PASS ] KVM support (API version: 12)
  [ PASS ] SEV enabled in KVM
  [ PASS ] SEV-ES enabled in KVM
  [ PASS ] SEV-SNP enabled in KVM

Compliance:
  [ PASS ] Memlock limit: Soft 202050342912 | Hard 202050342912
  [ PASS ] Memory alias check: Completed since last system update, no aliasing addresses
  [ PASS ] TCB version comparison: Platform TCB matches reported TCB

Installed Components:
  [ PASS ] QEMU [/usr/bin/qemu-system-x86_64]: 1:9.2.1+ds-1ubuntu5.2 (min: 6.0)
  [ WARN ] libvirt: Not installed (min: 4.5)
  [ PASS ] OVMF [/usr/share/ovmf/OVMF.amdsev.fd]: 2025.02-3ubuntu2.2
  [ PASS ] Kernel: 6.14.0-37-generic (min: 6.11)
  [ PASS ] SEV Firmware: 1.55 (min: 1.51)

─────────────────────────────────────────────
SUMMARY: 27 passed, 0 failed, 1 skipped (28 total)

OK
```

</details>

<details>
<summary><b>Error case (click to expand)</b></summary>

```
CPU Support:
  [ PASS ] AMD Processor
  [ PASS ] AMD EPYC Processor (Genoa)
  [ PASS ] SME support
  [ PASS ] SEV support
  [ PASS ] SEV-ES support
  [ PASS ] SEV-SNP support

CPU Info:
  [ PASS ] Physical address bits: 6
  [ PASS ] C-bit position: 51
  [ PASS ] Max encrypted guests: 1006
  [ PASS ] Min ASID for SEV-ES/SNP: 500
  [ PASS ] Page flush MSR: Enabled
  [ PASS ] VMPL support: 4 VMPLs

BIOS Configured:
  [ FAIL ] Firmware version: Permission denied (need sudo)
  [ FAIL ] SME enabled: MSR read failed (run: sudo modprobe msr)
  [ FAIL ] SNP enabled: MSR read failed (run: sudo modprobe msr)
  [ FAIL ] RMP addresses: MSR read failed (run: sudo modprobe msr)
  [ PASS ] ASID limits configured

Platform Initialized:
  [ FAIL ] /dev/sev readable: Permission denied (need sudo)
  [ FAIL ] /dev/sev writable: Permission denied (need sudo)
  [ FAIL ] SEV initialized: Permission denied (need sudo)
  [ FAIL ] SEV-ES initialized: Permission denied (need sudo)
  [ FAIL ] SNP initialized: Permission denied (need sudo)
  [ SKIP ] RMP initialized

KVM Config:
  [ FAIL ] KVM support: Permission denied (need sudo)
  [ SKIP ] SEV enabled in KVM
  [ SKIP ] SEV-ES enabled in KVM
  [ SKIP ] SEV-SNP enabled in KVM

Compliance:
  [ PASS ] Memlock limit: Soft 202050342912 | Hard 202050342912
  [ SKIP ] Memory alias check
  [ SKIP ] TCB version comparison

Installed Components:
  [ PASS ] QEMU [/usr/bin/qemu-system-x86_64]: 1:9.2.1+ds-1ubuntu5.2 (min: 6.0)
  [ WARN ] libvirt: Not installed (min: 4.5)
  [ PASS ] OVMF [/usr/share/ovmf/OVMF.amdsev.fd]: 2025.02-3ubuntu2.2
  [ PASS ] Kernel: 6.14.0-37-generic (min: 6.11)
  [ FAIL ] SEV Firmware: Permission denied (min: 1.51)

─────────────────────────────────────────────
SUMMARY: 15 passed, 10 failed, 6 skipped

DETECTED ISSUES:
  • Permission denied accessing /dev/sev
    → Run with sudo: sudo snphost ok

  • MSR read failures
    → Load MSR module: sudo modprobe msr

  • KVM not accessible
    → Load KVM modules: sudo modprobe kvm kvm_amd
```

</details>

**Pros:**
- Organizes by type of check rather than technology
- Easy to understand what category of testing is failing
- Matches reference documentation structure
- Clear separation between hardware checks, config, and runtime state

**Cons:**
- Tests for same technology (e.g., SEV-SNP) spread across multiple sections
- May be less intuitive for users thinking about "getting SNP working"

---

## Proposal 3: Traffic Light Format

**Change:** Color-coded category blocks with emoji status indicators.

<details open>
<summary><b>Success case (click to collapse)</b></summary>

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
            SEV-SNP Status Check
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🟢 Hardware ................................ OK
   AMD EPYC processor (Genoa), SNP supported, 4 VMPLs

🟢 Configuration ........................... OK
   SME, SNP enabled in BIOS, RMP allocated

🟢 Platform ................................ OK
   SEV Firmware 1.55, all features initialized

🟢 KVM .................................... OK
   KVM available, SEV/ES/SNP enabled

🟢 Resources ............................... OK
   Memlock: unlimited

🟢 Software ................................ OK
   QEMU 1:9.2.1, OVMF 2025.02, Kernel 6.14.0

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
RESULT: All checks passed (28/28)

✓ Your system is ready for SEV-SNP guests!
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

</details>

<details>
<summary><b>Error case (click to expand)</b></summary>

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
            SEV-SNP Status Check
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🟢 Hardware ................................ OK
   AMD EPYC processor (Genoa), SNP supported, 4 VMPLs

🔴 Configuration ........................ FAIL
   ✓ SME supported
   ✗ SME not enabled (BIOS required)
   ✗ SNP not enabled (BIOS required)

🔴 Platform ............................. FAIL
   ✗ Cannot access /dev/sev (need sudo)
   ⊘ Firmware version (skipped)
   ⊘ Initialization checks (skipped)

🟡 KVM ................................ WARN
   ✗ Cannot access /dev/kvm (need sudo)

🟢 Resources ............................. OK
   Memlock: unlimited

🟡 Software ............................ WARN
   ✓ QEMU: 1:9.2.1+ds-1ubuntu5.2
   ✓ OVMF: 2025.02-3ubuntu2.2
   ✓ Kernel: 6.14.0-37-generic
   ⊘ libvirt: Not installed
   ✗ SEV Firmware: Permission denied

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
RESULT: 3 categories need attention

ACTION REQUIRED:
1. Run with sudo: sudo snphost ok
2. Enable SNP in BIOS (see docs)

Run 'snphost ok --help' for details
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

</details>

**Pros:**
- Visual status at-a-glance with colors/emoji
- Groups related issues
- Clear action items
- User-friendly for beginners

**Cons:**
- Requires color/emoji support
- Less detailed than hierarchical formats
- May be too casual for enterprise use

---

## Proposal 3a: Traffic Light Format (Detailed)

**Change:** Color-coded category blocks with condensed successes and detailed failures.

<details open>
<summary><b>Success case (click to collapse)</b></summary>

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
            SEV-SNP Status Check
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🟢 Hardware Support ........................ OK
   ✓ AMD EPYC processor (Genoa)
   ✓ SME, SEV, SEV-ES, SEV-SNP supported

🟢 Firmware Support ........................ OK
   ✓ SEV Firmware 1.55
   ✓ SME, SNP enabled in BIOS
   ✓ RMP memory allocated

🟢 Platform Ready .......................... OK
   ✓ /dev/sev accessible
   ✓ SEV, SEV-ES, SNP initialized
   ✓ RMP initialized

🟢 KVM Support ............................. OK
   ✓ KVM available (API version: 12)
   ✓ SEV, SEV-ES, SEV-SNP enabled in KVM

🟢 Compliance .............................. OK
   ✓ Memlock limit: unlimited
   ✓ Memory alias check passed
   ✓ TCB versions aligned

🟢 Software ................................ OK
   ✓ QEMU 1:9.2.1, OVMF 2025.02, Kernel 6.14.0
   ✓ SEV Firmware 1.55
   ⊘ libvirt not installed (optional)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
RESULT: All checks passed (28/28)

✓ Your system is ready for SEV-SNP guests!
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

</details>

<details>
<summary><b>Error case - BIOS configuration (click to expand)</b></summary>

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
            SEV-SNP Status Check
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🟢 Hardware Support ........................ OK
   ✓ AMD EPYC processor (Genoa)
   ✓ SME, SEV, SEV-ES, SEV-SNP supported

🟡 Firmware Support .................... PARTIAL
   ✓ SEV Firmware 1.55
   ✓ SME enabled in BIOS
   ✗ SNP not enabled in BIOS
     → Enable in BIOS: CBS > CPU Common > SNP Memory Coverage
   ✗ RMP memory not allocated
     → Same BIOS setting enables RMP

🟡 Platform Ready ...................... PARTIAL
   ✓ /dev/sev accessible
   ✓ SEV, SEV-ES initialized
   ✗ SNP not initialized (BIOS configuration required)
   ⊘ RMP initialization skipped

🟢 KVM Support ............................. OK
   ✓ KVM available (API version: 12)
   ✓ SEV, SEV-ES enabled in KVM
   ⊘ SEV-SNP in KVM (requires SNP initialization)

🟢 Compliance .............................. OK
   ✓ Memlock limit: unlimited
   ⊘ Memory alias check (requires SNP)
   ⊘ TCB comparison (requires SNP)

🟢 Software ................................ OK
   ✓ QEMU 1:9.2.1, OVMF 2025.02, Kernel 6.14.0
   ✓ SEV Firmware 1.55
   ⊘ libvirt not installed (optional)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
RESULT: 2 categories need attention

ACTION REQUIRED:
1. Enable SNP in BIOS (CBS > CPU Common > SNP Memory Coverage)
2. Reboot system
3. Re-run: sudo snphost ok

Run 'snphost ok --help' for details
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

</details>

<details>
<summary><b>Error case - Permission issues (click to expand)</b></summary>

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
            SEV-SNP Status Check
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🟢 Hardware Support ........................ OK
   ✓ AMD EPYC processor (Genoa)
   ✓ SME, SEV, SEV-ES, SEV-SNP supported

🔴 Firmware Support ........................ FAIL
   ✗ SEV Firmware version: Permission denied
     → Run with sudo: sudo snphost ok
   ✗ SME enabled: MSR read failed
     → Load MSR module: sudo modprobe msr
   ✗ SNP enabled: MSR read failed
     → Load MSR module: sudo modprobe msr
   ✗ RMP addresses: MSR read failed
     → Load MSR module: sudo modprobe msr

🔴 Platform Ready .......................... FAIL
   ✗ /dev/sev not accessible: Permission denied
     → Run with sudo: sudo snphost ok
   ⊘ Initialization checks skipped

🔴 KVM Support ............................. FAIL
   ✗ KVM not accessible: Permission denied
     → Run with sudo: sudo snphost ok

🟢 Compliance .............................. OK
   ✓ Memlock limit configured

🟡 Software ............................ PARTIAL
   ✓ QEMU 1:9.2.1, OVMF 2025.02, Kernel 6.14.0
   ✗ SEV Firmware: Permission denied
   ⊘ libvirt not installed (optional)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
RESULT: 4 categories failed (permission issues)

ACTION REQUIRED:
1. Load MSR module: sudo modprobe msr
2. Re-run with sudo: sudo snphost ok

Run 'snphost ok --help' for details
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

</details>

**Pros:**
- Visual status at-a-glance with colors/emoji
- Condensed format when things work, detailed when they don't
- Clear action items grouped by issue
- Easy to scan for problems
- User-friendly for beginners and experts

**Cons:**
- Requires color/emoji support
- May be too casual for enterprise use
- More verbose than compact formats

---

## Proposal 4: Table Format (Structured)

**Change:** Use table format with box-drawing characters for hierarchical test results.

<details open>
<summary><b>Success case (click to collapse)</b></summary>

```
╭──────────────────────────────────────────────────────────────────┬────────┬──────────────────────────────────────────────────╮
│ Test Name                                                        │ Status │ Message                                          │
├──────────────────────────────────────────────────────────────────┼────────┼──────────────────────────────────────────────────┤
│ AMD CPU                                                          │ PASS   │                                                  │
│   └─ EPYC processor detected                                     │ PASS   │                                                  │
│   └─ SME supported                                               │ PASS   │                                                  │
│     └─ SME enabled in MSR                                        │ PASS   │ Enabled                                          │
│   └─ SEV supported                                               │ PASS   │                                                  │
│     └─ SEV firmware version                                      │ PASS   │ 1.55                                             │
│     └─ SEV-ES supported                                          │ PASS   │                                                  │
│       └─ SEV-ES initialized                                      │ PASS   │                                                  │
│     └─ SEV initialized                                           │ PASS   │ Initialized, no guests running                   │
│     └─ SEV-SNP supported                                         │ PASS   │                                                  │
│       └─ VMPL supported                                          │ PASS   │ 4 VMPLs available                                │
│       └─ SNP enabled in MSR                                      │ PASS   │ Enabled                                          │
│       └─ SNP initialized                                         │ PASS   │                                                  │
│         └─ RMP table base address                                │ PASS   │ 0x18278900000 - 0x183fc9fffff                    │
│         └─ RMP table initialized                                 │ PASS   │                                                  │
│         └─ Alias check                                           │ PASS   │ Completed since last system update, no aliasing  │
│                                                                  │        │ addresses                                        │
│     └─ Physical address bit reduction                            │ PASS   │ 6                                                │
│     └─ C-bit location                                            │ PASS   │ 51                                               │
│     └─ Number of encrypted guests supported simultaneously       │ PASS   │ 1006                                             │
│     └─ Minimum ASID value for SEV-ES-enabled, SEV-only disabled guest │ PASS   │ 500                                              │
│     └─ /dev/sev readable                                         │ PASS   │                                                  │
│     └─ /dev/sev writable                                         │ PASS   │                                                  │
│   └─ Page flush MSR                                              │ PASS   │ Enabled                                          │
│ KVM supported                                                    │ PASS   │ API version: 12                                  │
│   └─ SEV enabled in KVM                                          │ PASS   │                                                  │
│   └─ SEV-ES enabled in KVM                                       │ PASS   │                                                  │
│   └─ SEV-SNP enabled in KVM                                      │ PASS   │                                                  │
│ Memlock resource limit                                           │ PASS   │ Soft: 202050342912 | Hard: 202050342912          │
╰──────────────────────────────────────────────────────────────────┴────────┴──────────────────────────────────────────────────╯

Summary: 28 passed, 0 skipped, 0 failed (28 total)

Installed Components:
╭───────────────────────────────────────┬───────────────────────┬─────────────┬───────────────╮
│ Component                             │ Installed Version     │ Min Version │ Status        │
├───────────────────────────────────────┼───────────────────────┼─────────────┼───────────────┤
│ QEMU [/usr/bin/qemu-system-x86_64]    │ 1:9.2.1+ds-1ubuntu5.2 │ 6.0         │ Supported     │
│ libvirt                               │ N/A                   │ 4.5         │ Not Installed │
│ OVMF [/usr/share/ovmf/OVMF.amdsev.fd] │ 2025.02-3ubuntu2.2    │ N/A         │ Supported     │
│ Kernel                                │ 6.14.0-37-generic     │ 6.11        │ Supported     │
│ SEV Firmware                          │ 1.55                  │ 1.51        │ Supported     │
╰───────────────────────────────────────┴───────────────────────┴─────────────┴───────────────╯

TCB (Trusted Computing Base) Version Information:
╭─────────────────────┬─────────────────────╮
│ Platform Version    │ Reported Version    │
├─────────────────────┼─────────────────────┤
│ TCB Version:        │ TCB Version:        │
│   Microcode:   25   │   Microcode:   25   │
│   SNP:         27   │   SNP:         27   │
│   TEE:         0    │   TEE:         0    │
│   Boot Loader: 10   │   Boot Loader: 10   │
│   FMC:         None │   FMC:         None │
╰─────────────────────┴─────────────────────╯
```

</details>

<details>
<summary><b>Error case (click to expand)</b></summary>

```
╭──────────────────────────────────────────────────────────────────┬────────┬──────────────────────────────────────────────────╮
│ Test Name                                                        │ Status │ Message                                          │
├──────────────────────────────────────────────────────────────────┼────────┼──────────────────────────────────────────────────┤
│ AMD CPU                                                          │ PASS   │                                                  │
│   └─ EPYC processor detected                                     │ PASS   │                                                  │
│   └─ SME supported                                               │ PASS   │                                                  │
│     └─ SME enabled in MSR                                        │ FAIL   │ MSR read failed: Error Reading MSR               │
│   └─ SEV supported                                               │ PASS   │                                                  │
│     └─ SEV firmware version                                      │ FAIL   │ Failed to get SEV Platform Status unable to open │
│                                                                  │        │  /dev/sev                                        │
│     └─ SEV-ES supported                                          │ PASS   │                                                  │
│       └─ SEV-ES initialized                                      │ FAIL   │ Failed to get SEV Platform Status unable to open │
│                                                                  │        │  /dev/sev                                        │
│     └─ SEV initialized                                           │ FAIL   │ Failed to get SEV Platform Status unable to open │
│                                                                  │        │  /dev/sev                                        │
│     └─ SEV-SNP supported                                         │ PASS   │                                                  │
│       └─ VMPL supported                                          │ PASS   │ 4 VMPLs available                                │
│       └─ SNP enabled in MSR                                      │ FAIL   │ MSR read failed: Error Reading MSR               │
│       └─ SNP initialized                                         │ FAIL   │ Failed to get SNP Platform status unable to open │
│                                                                  │        │  /dev/sev                                        │
│         └─ RMP table base address                                │ SKIP   │                                                  │
│         └─ RMP table initialized                                 │ SKIP   │                                                  │
│         └─ RMP alias check complete                              │ SKIP   │                                                  │
│     └─ Physical address bit reduction                            │ PASS   │ 6                                                │
│     └─ C-bit location                                            │ PASS   │ 51                                               │
│     └─ Number of encrypted guests supported simultaneously       │ PASS   │ 1006                                             │
│     └─ Minimum ASID value for SEV-ES-enabled, SEV-only disabled guest │ PASS   │ 500                                              │
│     └─ /dev/sev readable                                         │ FAIL   │ Not readable: Permission denied (os error 13)    │
│     └─ /dev/sev writable                                         │ FAIL   │ Not writable: Permission denied (os error 13)    │
│   └─ Page flush MSR                                              │ PASS   │ Enabled                                          │
│ KVM supported                                                    │ FAIL   │ Error reading /dev/kvm: (Permission denied (os e │
│                                                                  │        │ rror 13))                                        │
│   └─ SEV enabled in KVM                                          │ SKIP   │                                                  │
│   └─ SEV-ES enabled in KVM                                       │ SKIP   │                                                  │
│   └─ SEV-SNP enabled in KVM                                      │ SKIP   │                                                  │
│ Memlock resource limit                                           │ PASS   │ Soft: 202050342912 | Hard: 202050342912          │
╰──────────────────────────────────────────────────────────────────┴────────┴──────────────────────────────────────────────────╯

Summary: 13 passed, 6 skipped, 9 failed (28 total)

Installed Components:
╭───────────────────────────────────────┬───────────────────────┬─────────────┬───────────────────╮
│ Component                             │ Installed Version     │ Min Version │ Status            │
├───────────────────────────────────────┼───────────────────────┼─────────────┼───────────────────┤
│ QEMU [/usr/bin/qemu-system-x86_64]    │ 1:9.2.1+ds-1ubuntu5.2 │ 6.0         │ Supported         │
│ libvirt                               │ N/A                   │ 4.5         │ Not Installed     │
│ OVMF [/usr/share/ovmf/OVMF.amdsev.fd] │ 2025.02-3ubuntu2.2    │ N/A         │ Supported         │
│ Kernel                                │ 6.14.0-37-generic     │ 6.11        │ Supported         │
│ SEV Firmware                          │ N/A                   │ 1.51        │ Permission Denied │
╰───────────────────────────────────────┴───────────────────────┴─────────────┴───────────────────╯
```

</details>

**Pros:**
- Very clean, professional appearance
- Easy to scan with clear columns
- Maintains full hierarchy with tree characters
- Additional tables provide valuable context
- Message column allows for detailed error info

**Cons:**
- Wider output (may not fit narrow terminals)
- More complex to implement (box-drawing characters)
- May not render correctly in all terminals/fonts

---

## Proposal 5: Compact Summary

**Change:** Show only failures and summary, suitable for CI/automation.

<details open>
<summary><b>Success case (click to collapse)</b></summary>

```
snphost ok: 28/28 passed

Software versions:
  ✓ QEMU 1:9.2.1+ds-1ubuntu5.2, OVMF 2025.02-3ubuntu2.2, Kernel 6.14.0-37-generic
  ✓ SEV Firmware 1.55
  ⊘ libvirt not installed (optional)
```

</details>

<details>
<summary><b>Error case (click to expand)</b></summary>

```
snphost ok: 13/28 passed, 9 failed, 6 skipped

FAILURES:
  ✗ SME enabled in MSR: MSR read failed
  ✗ SEV firmware version: Permission denied accessing /dev/sev
  ✗ SEV-ES initialized: Permission denied accessing /dev/sev
  ✗ SEV initialized: Permission denied accessing /dev/sev
  ✗ SNP enabled in MSR: MSR read failed
  ✗ SNP initialized: Permission denied accessing /dev/sev
  ✗ /dev/sev readable: Permission denied
  ✗ /dev/sev writable: Permission denied
  ✗ KVM supported: Permission denied accessing /dev/kvm

HINT: Run with sudo (9 permission errors detected)

Software versions:
  ✓ QEMU 1:9.2.1+ds-1ubuntu5.2, OVMF 2025.02-3ubuntu2.2, Kernel 6.14.0-37-generic
  ⊘ libvirt not installed
  ✗ SEV Firmware version check failed (permission denied)
```

</details>

**Pros:**
- Very concise, good for CI logs
- Focuses on what's wrong
- Shows all tests when using `-v`/`--verbose` flag
- Easy to grep/parse

**Cons:**
- Hides passing tests by default
- May not show enough detail for troubleshooting
- Requires verbose flag implementation

---

## Proposal 6: Check Prereqs

**Change:** Check prerequisites first, show clear error categories with counts.

<details open>
<summary><b>Success case (click to collapse)</b></summary>

```
[ PASS ] Running as root: Yes
[ PASS ] Running on baremetal: Yes
[ PASS ] MSR module loaded: Yes

All prerequisites met. Running full test suite...
[... full test output follows ...]
```

</details>

<details>
<summary><b>Error case (click to expand)</b></summary>

```
[ FAIL ] Running as root: No (many tests require sudo)
  - sudo snphost ok
[ PASS ] Running on baremetal: Yes
[ FAIL ] MSR module loaded: No
  - sudo modprobe msr

ERROR: Fix prerequisites above.
```

</details>

**Pros:**
- Immediately clear what the problem is
- Doesn't waste time running tests that will fail
- Provides exact command to fix issues

**Cons:**
- Major departure from current format
- Scripts expecting current format will break

---

## Implementation Options

### Format flags
```bash
snphost ok                     # Current format (default)
snphost ok --format=minimal    # Proposal 1 (inline hints)
snphost ok --format=grouped    # Proposal 2 (category summary)
snphost ok --format=table      # Proposal 4 (table with box chars)
snphost ok --format=compact    # Proposal 5 (failures only)
snphost ok --format=prereq     # Proposal 6 (check prereqs first)
snphost ok --format=json       # Machine-readable
```

### Verbosity Levels
```bash
snphost ok -q         # Quiet: only exit code
snphost ok            # Normal: show failures
snphost ok -v         # Verbose: show all tests
snphost ok -vv        # Very verbose: show debug info
```

## Oneline

**Change:** Ultra-compact single line summary with optional detailed output.

```bash
# Default (all pass)
$ snphost ok --oneline
✓ SEV-SNP ready (28/28 checks passed)

# With failures
$ snphost ok --oneline
✗ SEV-SNP not ready (13/28 passed, 9 failed - run with sudo)

# Quiet mode (exit code only)
$ snphost ok -q
$ echo $?
1
```

### JSON Output for Automation
```bash
snphost ok --json
```
```json
{
  "summary": {
    "passed": 19,
    "failed": 3,
    "skipped": 4,
    "total": 26
  },
  "prerequisites": {
    "sudo": false,
    "msr_module": false,
    "ccp_module": false
  },
  "info": {
    ...
  }
  "failures": [
    {
      "test": "SNP enabled",
      "category": "bios_config",
      "error": "MSR bit not set",
      "fix": "Enable SNP Memory Coverage in BIOS"
    }
  ]
}
```
