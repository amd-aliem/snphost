// SPDX-License-Identifier: Apache-2.0

use super::*;

use std::{
    arch::x86_64,
    fmt,
    fs::{self, File},
    mem::{transmute, MaybeUninit},
    os::unix::io::AsRawFd,
    process::Command,
    str::from_utf8,
};

use clap::Args;
use colorful::*;

use msru::{Accessor, Msr};

type TestFn = dyn Fn() -> TestResult;

// SEV generation-specific bitmasks.
const SEV_MASK: usize = 1;
const ES_MASK: usize = 1 << 1;
const SNP_MASK: usize = 1 << 2;

/// Output mode for the ok command
#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    Default,
    Short,
    Verbose,
    Json,
}

/// Arguments for the `ok` subcommand
#[derive(Args, Clone)]
pub struct OkArgs {
    /// Show only failures with summary counts
    #[arg(short, long)]
    short: bool,

    /// Show detailed test descriptions grouped by category
    #[arg(short, long)]
    verbose: bool,

    /// Output results as JSON
    #[arg(short, long)]
    json: bool,
}

impl OkArgs {
    fn output_mode(&self) -> OutputMode {
        if self.json {
            OutputMode::Json
        } else if self.verbose {
            OutputMode::Verbose
        } else if self.short {
            OutputMode::Short
        } else {
            OutputMode::Default
        }
    }
}

/// Accumulated result entry for post-processing by non-default renderers.
/// The `name` field contains the plain test name (no ANSI codes) for JSON/metadata lookup.
struct TestResultEntry {
    name: String,
    status: String, // "PASS", "FAIL", "SKIP"
    message: Option<String>,
    level: usize,
}

/// Category for grouping tests in verbose/JSON output
#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize)]
enum TestCategory {
    CpuSupport,
    CpuInfo,
    BiosConfigured,
    PlatformInitialized,
    KvmConfig,
    Compliance,
}

impl fmt::Display for TestCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TestCategory::CpuSupport => write!(f, "CPU Support"),
            TestCategory::CpuInfo => write!(f, "CPU Info"),
            TestCategory::BiosConfigured => write!(f, "BIOS Configured"),
            TestCategory::PlatformInitialized => write!(f, "Platform Initialized"),
            TestCategory::KvmConfig => write!(f, "KVM Config"),
            TestCategory::Compliance => write!(f, "Compliance"),
        }
    }
}

/// Metadata for a test, looked up by name
struct TestMetadata {
    category: TestCategory,
    label: &'static str,
    description: &'static str,
    fix_hint: &'static str,
}

const MSR_HINT: &str = "Load MSR kernel module: sudo modprobe msr";
const SUDO_HINT: &str = "Run with sudo: sudo snphost ok";

/// Returns the appropriate fix hint for a failed test. Overrides the
/// metadata hint when the failure is clearly an access/module issue
/// rather than a BIOS configuration problem.
fn effective_hint(meta: &TestMetadata, message: &Option<String>) -> &'static str {
    if let Some(m) = message {
        if m.contains("MSR read failed") || m.contains("Failed to read the desired MSR") {
            return MSR_HINT;
        }
        if m.contains("unable to open /dev/sev") || m.contains("Permission denied") {
            return SUDO_HINT;
        }
    }
    meta.fix_hint
}

/// Look up metadata for a test by its name.
/// Uses starts_with matching for tests whose names vary at runtime.
fn test_metadata(name: &str) -> TestMetadata {
    match name {
        "AMD CPU" => TestMetadata {
            category: TestCategory::CpuSupport,
            label: "(CPU)",
            description: "Checks CPU vendor string via CPUID is \"AuthenticAMD\"",
            fix_hint: "Requires AMD processor",
        },
        "Microcode support" => TestMetadata {
            category: TestCategory::CpuSupport,
            label: "(CPU)",
            description: "Verifies processor brand string contains \"EPYC\" (server-class CPU required)",
            fix_hint: "Need EPYC server-class CPU",
        },
        "Secure Memory Encryption (SME)" => TestMetadata {
            category: TestCategory::CpuSupport,
            label: "(CPU)",
            description: "Checks CPUID 0x8000001F EAX bit 0 for SME hardware support",
            fix_hint: "Need EPYC Naples+",
        },
        "SME" => TestMetadata {
            category: TestCategory::BiosConfigured,
            label: "(BIOS)",
            description: "Reads MSR 0xC0010010 (SYSCFG) bit 23 to verify SME enabled at system level",
            fix_hint: "BIOS: CBS > CPU Common > SMEE. Run: sudo modprobe msr",
        },
        "Secure Encrypted Virtualization (SEV)" => TestMetadata {
            category: TestCategory::CpuSupport,
            label: "(CPU)",
            description: "Checks CPUID 0x8000001F EAX bit 1 for SEV hardware support",
            fix_hint: "Need EPYC with SEV",
        },
        "SEV firmware version" => TestMetadata {
            category: TestCategory::BiosConfigured,
            label: "",
            description: "Queries /dev/sev PLATFORM_STATUS for firmware version (requires >= 1.51 for SNP)",
            fix_hint: "Run with sudo. Update BIOS for firmware >= 1.51",
        },
        "Encrypted State (SEV-ES)" => TestMetadata {
            category: TestCategory::CpuSupport,
            label: "(CPU)",
            description: "Checks CPUID 0x8000001F EAX bit 3 for SEV-ES hardware support",
            fix_hint: "Need EPYC Rome+",
        },
        "SEV-ES initialized" => TestMetadata {
            category: TestCategory::PlatformInitialized,
            label: "(FW Ready)",
            description: "Queries SEV platform status flags bit 8 for SEV-ES initialization",
            fix_hint: "Run with sudo. modprobe kvm_amd sev-es=1",
        },
        "SEV initialized" => TestMetadata {
            category: TestCategory::PlatformInitialized,
            label: "(FW Ready)",
            description: "Queries SEV platform status state field (must be Initialized or Working)",
            fix_hint: "Run with sudo. modprobe kvm_amd sev=1",
        },
        "Secure Nested Paging (SEV-SNP)" => TestMetadata {
            category: TestCategory::CpuSupport,
            label: "(CPU)",
            description: "Checks CPUID 0x8000001F EAX bit 4 for SEV-SNP hardware support",
            fix_hint: "Need EPYC Milan+",
        },
        "VM Permission Levels" => TestMetadata {
            category: TestCategory::CpuSupport,
            label: "(CPU)",
            description: "Checks CPUID 0x8000001F EAX bit 5 for VMPL hardware support",
            fix_hint: "Check BIOS update",
        },
        "Number of VMPLs" => TestMetadata {
            category: TestCategory::CpuInfo,
            label: "",
            description: "Reads CPUID 0x8000001F EBX bits 15:12 for VMPL count (expected: 4)",
            fix_hint: "(informational)",
        },
        "SNP" | "SEV-SNP" => TestMetadata {
            category: TestCategory::BiosConfigured,
            label: "(BIOS)",
            description: "Reads MSR 0xC0010010 (SYSCFG) bit 24 to verify SNP enabled at system level",
            fix_hint: "BIOS: CBS > CPU Common > SNP Memory Coverage. Run: sudo modprobe msr",
        },
        "SNP initialized" => TestMetadata {
            category: TestCategory::PlatformInitialized,
            label: "(FW Ready)",
            description: "Queries SNP_PLATFORM_STATUS state field = 1 (INIT state)",
            fix_hint: "Run with sudo. Need kernel 6.11+. modprobe kvm_amd sev_snp=1",
        },
        "RMP table initialized" => TestMetadata {
            category: TestCategory::PlatformInitialized,
            label: "(FW Ready)",
            description: "Queries SNP platform status IS_RMP_INIT bit",
            fix_hint: "Run with sudo. Need CONFIG_KVM_AMD_SEV=y. Reboot if firmware updated",
        },
        "Alias check" => TestMetadata {
            category: TestCategory::Compliance,
            label: "(Compliance)",
            description: "Queries SNP platform status ALIAS_CHECK_COMPLETE bit (CVE-2024-21944 mitigation)",
            fix_hint: "Update firmware/BIOS per AMD-SB-3015. Reboot required",
        },
        "Physical address bit reduction" => TestMetadata {
            category: TestCategory::CpuInfo,
            label: "",
            description: "Reads CPUID 0x8000001F EBX bits 11:6 for PA bit reduction value",
            fix_hint: "(informational)",
        },
        "C-bit location" => TestMetadata {
            category: TestCategory::CpuInfo,
            label: "",
            description: "Reads CPUID 0x8000001F EBX bits 5:0 for encryption bit position in page tables",
            fix_hint: "(informational)",
        },
        "Number of encrypted guests supported simultaneously" => TestMetadata {
            category: TestCategory::CpuInfo,
            label: "",
            description: "Reads CPUID 0x8000001F ECX for maximum encrypted guest count",
            fix_hint: "(informational)",
        },
        "Minimum ASID value for SEV-enabled, SEV-ES disabled guest" => TestMetadata {
            category: TestCategory::CpuInfo,
            label: "",
            description: "Reads CPUID 0x8000001F EDX for minimum SEV-only ASID value",
            fix_hint: "(informational)",
        },
        "/dev/sev readable" => TestMetadata {
            category: TestCategory::PlatformInitialized,
            label: "",
            description: "Attempts to open /dev/sev device for reading",
            fix_hint: "Run with sudo. modprobe ccp. Must be baremetal",
        },
        "/dev/sev writable" => TestMetadata {
            category: TestCategory::PlatformInitialized,
            label: "",
            description: "Attempts to open /dev/sev device for writing",
            fix_hint: "Run with sudo. Must be baremetal",
        },
        "Memlock resource limit" => TestMetadata {
            category: TestCategory::Compliance,
            label: "(Compliance)",
            description: "Reads RLIMIT_MEMLOCK soft and hard limits via getrlimit syscall",
            fix_hint: "Set memlock unlimited in /etc/security/limits.conf",
        },
        _ if name.starts_with("Page flush MSR") => TestMetadata {
            category: TestCategory::CpuInfo,
            label: "",
            description: "Checks CPUID 0x8000001F EAX bit 2 for page flush MSR optimization support",
            fix_hint: "(informational)",
        },
        _ if name.starts_with("KVM supported") || name == "KVM Support" => TestMetadata {
            category: TestCategory::KvmConfig,
            label: "(KVM)",
            description: "Opens /dev/kvm and queries KVM API version via ioctl",
            fix_hint: "Run with sudo. modprobe kvm kvm_amd. Enable SVM in BIOS",
        },
        "SEV enabled in KVM" => TestMetadata {
            category: TestCategory::KvmConfig,
            label: "(KVM)",
            description: "Reads /sys/module/kvm_amd/parameters/sev for \"1\" or \"Y\"",
            fix_hint: "options kvm_amd sev=1 in /etc/modprobe.d/kvm.conf",
        },
        "SEV-ES enabled in KVM" => TestMetadata {
            category: TestCategory::KvmConfig,
            label: "(KVM)",
            description: "Reads /sys/module/kvm_amd/parameters/sev_es for \"1\" or \"Y\"",
            fix_hint: "options kvm_amd sev-es=1 in /etc/modprobe.d/kvm.conf",
        },
        "SEV-SNP enabled in KVM" => TestMetadata {
            category: TestCategory::KvmConfig,
            label: "(KVM)",
            description: "Reads /sys/module/kvm_amd/parameters/sev_snp for \"1\" or \"Y\"",
            fix_hint: "options kvm_amd sev-snp=1 in /etc/modprobe.d/kvm.conf. Need kernel 6.11+",
        },
        _ if name.starts_with("Comparing TCB") || name.starts_with("Compare TCB") => TestMetadata {
            category: TestCategory::Compliance,
            label: "(Compliance)",
            description: "Compares platform_tcb_version with reported_tcb_version from SNP_PLATFORM_STATUS",
            fix_hint: "Run: sudo snphost commit or sudo snphost config set-reported-tcb",
        },
        _ if name.starts_with("Reading RMP table") || name.starts_with("RMP table address") || name.starts_with("Read RMP") => TestMetadata {
            category: TestCategory::BiosConfigured,
            label: "(BIOS)",
            description: "Reads MSRs 0xC0010132 and 0xC0010133 for RMP base/end addresses",
            fix_hint: "Enable SNP Memory Coverage in BIOS. Run: sudo modprobe msr",
        },
        _ => TestMetadata {
            category: TestCategory::CpuInfo,
            label: "",
            description: "",
            fix_hint: "",
        },
    }
}

struct Test {
    name: &'static str,
    gen_mask: usize,
    run: Box<TestFn>,
    sub: Vec<Test>,
}

struct TestResult {
    name: String,
    stat: TestState,
    mesg: Option<String>,
}

#[derive(PartialEq, Eq)]
enum TestState {
    Pass,
    Skip,
    Fail,
}

enum SevGeneration {
    Sev,
    Es,
    Snp,
}

impl fmt::Display for TestState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TestState::Pass => format!("{}", "PASS".green()),
            TestState::Skip => format!("{}", "SKIP".yellow()),
            TestState::Fail => format!("{}", "FAIL".red()),
        };

        write!(f, "{}", s)
    }
}

enum SnpStatusTest {
    Tcb,
    Rmp,
    AliasCheck,
    Snp,
}

enum SevStatusTests {
    Sev,
    Firmware,
    SevEs,
}

impl fmt::Display for SnpStatusTest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SnpStatusTest::Tcb => "Comparing TCB values",
            SnpStatusTest::Rmp => "RMP table initialized",
            SnpStatusTest::AliasCheck => "Alias check",
            SnpStatusTest::Snp => "SNP initialized",
        };
        write!(f, "{}", s)
    }
}

impl fmt::Display for SevStatusTests {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SevStatusTests::Sev => "SEV initialized",
            SevStatusTests::Firmware => "SEV firmware version",
            SevStatusTests::SevEs => "SEV-ES initialized",
        };
        write!(f, "{}", s)
    }
}

fn collect_tests() -> Vec<Test> {
    let tests = vec![
        Test {
            name: "AMD CPU",
            gen_mask: SEV_MASK,
            run: Box::new(|| {
                let res = unsafe { x86_64::__cpuid(0x0000_0000) };
                let name: [u8; 12] = unsafe { transmute([res.ebx, res.edx, res.ecx]) };
                let name = from_utf8(&name[..]).unwrap_or("ERROR_FOUND");

                let stat = if name == "AuthenticAMD" {
                    TestState::Pass
                } else {
                    TestState::Fail
                };

                TestResult {
                    name: "AMD CPU".to_string(),
                    stat,
                    mesg: None,
                }
            }),
            sub: vec![
                Test {
                    name: "Microcode support",
                    gen_mask: SEV_MASK,
                    run: Box::new(|| {
                        let cpu_name = {
                            let mut bytestr = Vec::with_capacity(48);
                            for cpuid in 0x8000_0002_u32..=0x8000_0004_u32 {
                                let cpuid = unsafe { x86_64::__cpuid(cpuid) };
                                let mut bytes: Vec<u8> =
                                    [cpuid.eax, cpuid.ebx, cpuid.ecx, cpuid.edx]
                                        .iter()
                                        .flat_map(|r| r.to_le_bytes().to_vec())
                                        .collect();
                                bytestr.append(&mut bytes);
                            }
                            String::from_utf8(bytestr)
                                .unwrap_or_else(|_| "ERROR_FOUND".to_string())
                                .trim()
                                .to_string()
                        };

                        let stat = if cpu_name.to_uppercase().contains("EPYC") {
                            TestState::Pass
                        } else {
                            TestState::Fail
                        };

                        TestResult {
                            name: "Microcode support".to_string(),
                            stat,
                            mesg: None,
                        }
                    }),
                    sub: vec![],
                },
                Test {
                    name: "Secure Memory Encryption (SME)",
                    gen_mask: SEV_MASK,
                    run: Box::new(|| {
                        let res = unsafe { x86_64::__cpuid(0x8000_001f) };

                        let stat = if (res.eax & 0x1) != 0 {
                            TestState::Pass
                        } else {
                            TestState::Fail
                        };

                        TestResult {
                            name: "Secure Memory Encryption (SME)".to_string(),
                            stat,
                            mesg: None,
                        }
                    }),
                    sub: vec![Test {
                        name: "SME",
                        gen_mask: SEV_MASK,
                        run: Box::new(sme_test),
                        sub: vec![],
                    }],
                },
                Test {
                    name: "Secure Encrypted Virtualization (SEV)",
                    gen_mask: SEV_MASK,
                    run: Box::new(|| {
                        let res = unsafe { x86_64::__cpuid(0x8000_001f) };

                        let stat = if ((res.eax & 0x1) << 1) != 0 {
                            TestState::Pass
                        } else {
                            TestState::Fail
                        };

                        TestResult {
                            name: "Secure Encrypted Virtualization (SEV)".to_string(),
                            stat,
                            mesg: None,
                        }
                    }),
                    sub: vec![
                        Test {
                            name: "SEV Firmware Version",
                            gen_mask: SNP_MASK,
                            run: Box::new(|| sev_ioctl(SevStatusTests::Firmware)),
                            sub: vec![],
                        },
                        Test {
                            name: "Encrypted State (SEV-ES)",
                            gen_mask: ES_MASK,
                            run: Box::new(|| {
                                let res = unsafe { x86_64::__cpuid(0x8000_001f) };

                                let stat = if ((res.eax & 0x1) << 3) != 0 {
                                    TestState::Pass
                                } else {
                                    TestState::Fail
                                };

                                TestResult {
                                    name: "Encrypted State (SEV-ES)".to_string(),
                                    stat,
                                    mesg: None,
                                }
                            }),
                            sub: vec![Test {
                                name: "SEV-ES initialized",
                                gen_mask: ES_MASK,
                                run: Box::new(|| sev_ioctl(SevStatusTests::SevEs)),
                                sub: vec![],
                            }],
                        },
                        Test {
                            name: "SEV initialized",
                            gen_mask: SNP_MASK,
                            run: Box::new(|| sev_ioctl(SevStatusTests::Sev)),
                            sub: vec![],
                        },
                        Test {
                            name: "Secure Nested Paging (SEV-SNP)",
                            gen_mask: SNP_MASK,
                            run: Box::new(|| {
                                let res = unsafe { x86_64::__cpuid(0x8000_001f) };

                                let stat = if ((res.eax & 0x1) << 4) != 0 {
                                    TestState::Pass
                                } else {
                                    TestState::Fail
                                };

                                TestResult {
                                    name: "Secure Nested Paging (SEV-SNP)".to_string(),
                                    stat,
                                    mesg: None,
                                }
                            }),
                            sub: vec![
                                Test {
                                    name: "VM Permission Levels",
                                    gen_mask: SNP_MASK,
                                    run: Box::new(|| {
                                        let res = unsafe { x86_64::__cpuid(0x8000_001f) };

                                        let stat = if ((res.eax & 0x1) << 5) != 0 {
                                            TestState::Pass
                                        } else {
                                            TestState::Fail
                                        };

                                        TestResult {
                                            name: "VM Permission Levels".to_string(),
                                            stat,
                                            mesg: None,
                                        }
                                    }),
                                    sub: vec![Test {
                                        name: "Number of VMPLs",
                                        gen_mask: SNP_MASK,
                                        run: Box::new(|| {
                                            let res = unsafe { x86_64::__cpuid(0x8000_001f) };
                                            let num_vmpls = (res.ebx & 0xF000) >> 12;

                                            TestResult {
                                                name: "Number of VMPLs".to_string(),
                                                stat: TestState::Pass,
                                                mesg: Some(format!("{}", num_vmpls)),
                                            }
                                        }),
                                        sub: vec![],
                                    }],
                                },
                                Test {
                                    name: "SEV-SNP",
                                    gen_mask: SNP_MASK,
                                    run: Box::new(snp_test),
                                    sub: vec![],
                                },
                                Test {
                                    name: "SNP initialized",
                                    gen_mask: SNP_MASK,
                                    run: Box::new(|| snp_ioctl(SnpStatusTest::Snp)),
                                    sub: vec![
                                        Test {
                                            name: "Read RMP tables",
                                            gen_mask: SNP_MASK,
                                            run: Box::new(get_rmp_address),
                                            sub: vec![],
                                        },
                                        Test {
                                            name: "RMP table initialized",
                                            gen_mask: SNP_MASK,
                                            run: Box::new(|| snp_ioctl(SnpStatusTest::Rmp)),
                                            sub: vec![],
                                        },
                                        Test {
                                            name: "Alias check",
                                            gen_mask: SNP_MASK,
                                            run: Box::new(|| snp_ioctl(SnpStatusTest::AliasCheck)),
                                            sub: vec![],
                                        },
                                    ],
                                },
                            ],
                        },
                        Test {
                            name: "Physical address bit reduction",
                            gen_mask: SEV_MASK,
                            run: Box::new(|| {
                                let res = unsafe { x86_64::__cpuid(0x8000_001f) };
                                let field = (res.ebx & 0b1111_1100_0000) >> 6;

                                TestResult {
                                    name: "Physical address bit reduction".to_string(),
                                    stat: TestState::Pass,
                                    mesg: Some(format!("{}", field)),
                                }
                            }),
                            sub: vec![],
                        },
                        Test {
                            name: "C-bit location",
                            gen_mask: SEV_MASK,
                            run: Box::new(|| {
                                let res = unsafe { x86_64::__cpuid(0x8000_001f) };
                                let field = res.ebx & 0b11_1111;

                                TestResult {
                                    name: "C-bit location".to_string(),
                                    stat: TestState::Pass,
                                    mesg: Some(format!("{}", field)),
                                }
                            }),
                            sub: vec![],
                        },
                        Test {
                            name: "Number of encrypted guests supported simultaneously",
                            gen_mask: SEV_MASK,
                            run: Box::new(|| {
                                let res = unsafe { x86_64::__cpuid(0x8000_001f) };
                                let field = res.ecx;

                                TestResult {
                                    name: "Number of encrypted guests supported simultaneously"
                                        .to_string(),
                                    stat: TestState::Pass,
                                    mesg: Some(format!("{}", field)),
                                }
                            }),
                            sub: vec![],
                        },
                        Test {
                            name: "Minimum ASID value for SEV-enabled, SEV-ES disabled guest",
                            gen_mask: SEV_MASK,
                            run: Box::new(|| {
                                let res = unsafe { x86_64::__cpuid(0x8000_001f) };
                                let field = res.edx;

                                TestResult {
                                    name:
                                        "Minimum ASID value for SEV-enabled, SEV-ES disabled guest"
                                            .to_string(),
                                    stat: TestState::Pass,
                                    mesg: Some(format!("{}", field)),
                                }
                            }),
                            sub: vec![],
                        },
                        Test {
                            name: "/dev/sev readable",
                            gen_mask: SEV_MASK,
                            run: Box::new(dev_sev_r),
                            sub: vec![],
                        },
                        Test {
                            name: "/dev/sev writable",
                            gen_mask: SEV_MASK,
                            run: Box::new(dev_sev_w),
                            sub: vec![],
                        },
                    ],
                },
                Test {
                    name: "Page flush MSR",
                    gen_mask: SEV_MASK,
                    run: Box::new(|| {
                        let res = unsafe { x86_64::__cpuid(0x8000_001f) };

                        let enabled = (res.eax & (1 << 2)) != 0;
                        let msr_flag = if enabled {
                            "ENABLED".green()
                        } else {
                            "DISABLED".yellow()
                        };

                        let name = format!("Page flush MSR: {}", msr_flag);

                        TestResult {
                            name,
                            /*
                             * Page flush MSR can be enabled/disabled.
                             * Therefore, if the flag is disabled, it doesn't
                             * necessarily mean that Page flush MSR *isn't*
                             * supported, but rather that it is supported yet
                             * currently disabled. So instead of returning
                             * TestState::Fail (indicating that Page flush MSR
                             * isn't supported), return TestState::Pass and
                             * indicate to the caller whether it is enabled or
                             * disabled.
                             */
                            stat: TestState::Pass,
                            mesg: Some(if enabled { "ENABLED" } else { "DISABLED" }.to_string()),
                        }
                    }),
                    sub: vec![],
                },
            ],
        },
        Test {
            name: "KVM Support",
            gen_mask: SEV_MASK,
            run: Box::new(has_kvm_support),
            sub: vec![
                Test {
                    name: "SEV enabled in KVM",
                    gen_mask: SEV_MASK,
                    run: Box::new(|| sev_enabled_in_kvm(SevGeneration::Sev)),
                    sub: vec![],
                },
                Test {
                    name: "SEV-ES enabled in KVM",
                    gen_mask: ES_MASK,
                    run: Box::new(|| sev_enabled_in_kvm(SevGeneration::Es)),
                    sub: vec![],
                },
                Test {
                    name: "SEV-SNP enabled in KVM",
                    gen_mask: SNP_MASK,
                    run: Box::new(|| sev_enabled_in_kvm(SevGeneration::Snp)),
                    sub: vec![],
                },
            ],
        },
        Test {
            name: "memlock limit",
            gen_mask: SEV_MASK,
            run: Box::new(memlock_rlimit),
            sub: vec![],
        },
        Test {
            name: "Compare TCB values",
            gen_mask: SNP_MASK,
            run: Box::new(|| snp_ioctl(SnpStatusTest::Tcb)),
            sub: vec![],
        },
    ];

    tests
}

const INDENT: usize = 2;

pub fn cmd(quiet: bool, args: OkArgs) -> Result<()> {
    let tests = collect_tests();
    let mode = args.output_mode();
    let suppress_print = quiet || mode != OutputMode::Default;

    let mut entries: Vec<TestResultEntry> = Vec::new();
    let passed = run_test(&tests, 0, suppress_print, SEV_MASK | ES_MASK | SNP_MASK, &mut entries);

    let sw_versions = collect_software_versions();

    if !quiet {
        match mode {
            OutputMode::Default => {
                print_software_versions(&sw_versions);
            }
            _ => render_output(mode, &entries, &sw_versions, passed),
        }
    }

    if passed {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "One or more tests in snphost ok reported a failure"
        ))
    }
}

fn run_test(
    tests: &[Test],
    level: usize,
    quiet: bool,
    mask: usize,
    entries: &mut Vec<TestResultEntry>,
) -> bool {
    let mut passed = true;

    for t in tests {
        // Skip tests that aren't included in the specified generation.
        if (t.gen_mask & mask) != t.gen_mask {
            test_gen_not_included(t, level, quiet);
            accumulate_skip(t, level, entries);
            continue;
        }

        let res = (t.run)();
        emit_result(&res, level, quiet);
        let status_str = match res.stat {
            TestState::Pass => "PASS",
            TestState::Fail => "FAIL",
            TestState::Skip => "SKIP",
        };
        entries.push(TestResultEntry {
            name: t.name.to_string(),
            status: status_str.to_string(),
            message: res.mesg.clone(),
            level,
        });
        match res.stat {
            TestState::Pass => {
                if !run_test(&t.sub, level + INDENT, quiet, mask, entries) {
                    passed = false;
                }
            }
            TestState::Fail => {
                passed = false;
                emit_skip(&t.sub, level + INDENT, quiet);
                accumulate_skip_all(&t.sub, level + INDENT, entries);
            }
            // Skipped tests are marked as skip before recursing. They are just emitted and not actually processed.
            TestState::Skip => unreachable!(),
        }
    }

    passed
}

/// Accumulate a single skipped test and its subtests
fn accumulate_skip(test: &Test, level: usize, entries: &mut Vec<TestResultEntry>) {
    entries.push(TestResultEntry {
        name: test.name.to_string(),
        status: "SKIP".to_string(),
        message: None,
        level,
    });
    accumulate_skip_all(&test.sub, level + INDENT, entries);
}

/// Accumulate all tests as skipped (recursive)
fn accumulate_skip_all(tests: &[Test], level: usize, entries: &mut Vec<TestResultEntry>) {
    for t in tests {
        entries.push(TestResultEntry {
            name: t.name.to_string(),
            status: "SKIP".to_string(),
            message: None,
            level,
        });
        accumulate_skip_all(&t.sub, level + INDENT, entries);
    }
}

/// Renderer for non-default output modes
fn render_output(
    mode: OutputMode,
    entries: &[TestResultEntry],
    sw_versions: &[SoftwareVersion],
    passed: bool,
) {
    match mode {
        OutputMode::Short => render_short(entries, sw_versions, passed),
        OutputMode::Verbose => render_verbose(entries, sw_versions),
        OutputMode::Json => render_json(entries, sw_versions, passed),
        OutputMode::Default => {} // handled inline
    }
}

fn render_short(entries: &[TestResultEntry], sw_versions: &[SoftwareVersion], _passed: bool) {
    let fail_count = entries.iter().filter(|e| e.status == "FAIL").count();
    let pass_count = entries.iter().filter(|e| e.status == "PASS").count();
    let skip_count = entries.iter().filter(|e| e.status == "SKIP").count();
    let total = entries.len();

    if fail_count > 0 {
        println!("{}", "Failures:".red());
        for e in entries {
            if e.status == "FAIL" {
                let meta = test_metadata(&e.name);
                let label = if meta.label.is_empty() {
                    String::new()
                } else {
                    format!(" {}", meta.label)
                };
                let msg = match &e.message {
                    Some(m) => format!(": {}", m),
                    None => String::new(),
                };
                let hint_str = effective_hint(&meta, &e.message);
                let hint = if hint_str.is_empty() {
                    String::new()
                } else {
                    format!("\n    ^ {} {}", "Hint:".blue(), hint_str)
                };
                println!("  {} {}{}{}{}", "FAIL".red(), e.name, label, msg, hint);
            }
        }
    }

    // Software version issues
    let sw_issues: Vec<&SoftwareVersion> = sw_versions
        .iter()
        .filter(|v| v.status != "ok" && v.status != "optional_missing")
        .collect();
    if !sw_issues.is_empty() {
        println!("\n{}", "Software issues:".yellow());
        for v in sw_issues {
            let ver_str = v.version.as_deref().unwrap_or("not found");
            println!("  {} {}: {} ({})", "WARN".yellow(), v.name, ver_str, v.detail);
        }
    }

    // Show skipped items
    let skipped_tests: Vec<&TestResultEntry> = entries.iter().filter(|e| e.status == "SKIP").collect();
    let sw_skipped: Vec<&SoftwareVersion> = sw_versions
        .iter()
        .filter(|v| v.status == "optional_missing")
        .collect();
    if !skipped_tests.is_empty() || !sw_skipped.is_empty() {
        println!("\n{}:", "SKIPPED".yellow());
        for s in &skipped_tests {
            println!("  [{}] {}", "SKIP".yellow(), s.name);
        }
        for v in &sw_skipped {
            println!("  [{}] {} (Software)", "SKIP".yellow(), v.name);
        }
    }

    let sw_pass = sw_versions.iter().filter(|v| v.status == "ok").count();
    let sw_fail = sw_versions.iter().filter(|v| v.status == "too_old" || v.status == "missing").count();
    let sw_skip = sw_versions.iter().filter(|v| v.status == "optional_missing").count();

    println!(
        "\n{} tests: {} passed, {} failed, {} skipped",
        total + sw_versions.len(),
        pass_count + sw_pass,
        fail_count + sw_fail,
        skip_count + sw_skip,
    );
    if fail_count == 0 && sw_fail == 0 {
        println!("{}", "All tests passed.".green());
    }
}

fn render_verbose(entries: &[TestResultEntry], sw_versions: &[SoftwareVersion]) {
    // Group entries by category, preserving order of first appearance
    let categories = [
        TestCategory::CpuSupport,
        TestCategory::CpuInfo,
        TestCategory::BiosConfigured,
        TestCategory::PlatformInitialized,
        TestCategory::KvmConfig,
        TestCategory::Compliance,
    ];

    for cat in &categories {
        let cat_entries: Vec<&TestResultEntry> = entries
            .iter()
            .filter(|e| test_metadata(&e.name).category == *cat)
            .collect();
        if cat_entries.is_empty() {
            continue;
        }
        println!("\n=== {} ===", cat);
        for e in &cat_entries {
            let meta = test_metadata(&e.name);
            let status_colored = match e.status.as_str() {
                "PASS" => format!("{}", "PASS".green()),
                "FAIL" => format!("{}", "FAIL".red()),
                _ => format!("{}", "SKIP".yellow()),
            };
            let msg = match &e.message {
                Some(m) => {
                    // Indent continuation lines to align under the test name
                    let indented = m.replace('\n', "\n             ");
                    format!(": {}", indented.trim())
                }
                None => String::new(),
            };
            println!("  [ {:^4} ] {}{}", status_colored, e.name, msg);
            if !meta.description.is_empty() {
                println!("           {}", meta.description);
            }
            let hint_str = effective_hint(&meta, &e.message);
            if e.status == "FAIL" && !hint_str.is_empty() {
                println!(
                    "           {} {}",
                    "Recommended:".yellow(),
                    hint_str
                );
            }
        }
    }

    // Software versions
    println!("\n=== Installed Components ===");
    for v in sw_versions {
        let stat = match v.status.as_str() {
            "ok" => format!("{}", "PASS".green()),
            "too_old" => format!("{}", "FAIL".red()),
            "missing" => format!("{}", "SKIP".yellow()),
            "optional_missing" => format!("{}", "SKIP".yellow()),
            _ => format!("{}", "SKIP".yellow()),
        };
        let ver_str = v.version.as_deref().unwrap_or("not found");
        // Extract min version from detail (e.g. "kernel >= 6.11")
        let min_str = if let Some(pos) = v.detail.find(">=") {
            format!(" (min: {})", v.detail[pos + 2..].trim())
        } else {
            String::new()
        };
        let detail = match v.status.as_str() {
            "missing" | "optional_missing" => format!("Not installed{}", min_str),
            _ => format!("{}{}", ver_str, min_str),
        };
        println!("  [ {:^4} ] {}: {}", stat, v.name, detail);
    }

    // Detected Issues summary
    let issues: Vec<&TestResultEntry> = entries
        .iter()
        .filter(|e| e.status == "FAIL")
        .collect();
    let sw_issues: Vec<&SoftwareVersion> = sw_versions
        .iter()
        .filter(|v| v.status == "too_old" || v.status == "missing")
        .collect();

    if !issues.is_empty() || !sw_issues.is_empty() {
        println!("\n{}", "=== DETECTED ISSUES ===".red());
        for (i, e) in issues.iter().enumerate() {
            let meta = test_metadata(&e.name);
            let hint_str = effective_hint(&meta, &e.message);
            println!("  {}. {} [FAIL]", i + 1, e.name);
            if !hint_str.is_empty() {
                println!("     {} {}", "Hint:".blue(), hint_str);
            }
        }
        for v in &sw_issues {
            let ver_str = v.version.as_deref().unwrap_or("not found");
            println!(
                "  - {} {} ({})",
                v.name, ver_str, v.detail
            );
        }
        println!();
        println!("For detailed troubleshooting, see: https://github.com/virtee/snphost/tree/main/docs/snphost-ok-reference.md");
    } else {
        println!("\n{}", "No issues detected.".green());
    }
}

fn category_to_snake(cat: &TestCategory) -> &'static str {
    match cat {
        TestCategory::CpuSupport => "cpu_support",
        TestCategory::CpuInfo => "cpu_info",
        TestCategory::BiosConfigured => "bios_configured",
        TestCategory::PlatformInitialized => "platform_initialized",
        TestCategory::KvmConfig => "kvm_config",
        TestCategory::Compliance => "compliance",
    }
}

fn entry_to_json(e: &TestResultEntry) -> serde_json::Value {
    let meta = test_metadata(&e.name);
    let is_tcb = e.name.starts_with("Comparing TCB") || e.name.starts_with("Compare TCB");

    let mut obj = serde_json::json!({
        "name": e.name,
        "status": e.status.to_lowercase(),
        "category": category_to_snake(&meta.category),
    });
    if !meta.label.is_empty() {
        // Strip parentheses from label for JSON
        let label = meta.label.trim_start_matches('(').trim_end_matches(')');
        obj["label"] = serde_json::json!(label);
    }
    if is_tcb {
        // Short message instead of raw multi-line dump
        if let Some(m) = &e.message {
            obj["message"] = if m.starts_with("TCB versions match") {
                serde_json::json!("TCB versions match")
            } else {
                serde_json::json!("TCB versions do NOT match")
            };
        }
        // Add structured TCB data
        if let Ok(status) = snp_platform_status() {
            let tcb = &status.platform_tcb_version;
            let rtcb = &status.reported_tcb_version;
            obj["tcb"] = serde_json::json!({
                "versions_match": tcb == rtcb,
                "platform": {
                    "microcode": tcb.microcode,
                    "snp": tcb.snp,
                    "tee": tcb.tee,
                    "boot_loader": tcb.bootloader,
                    "fmc": tcb.fmc,
                },
                "reported": {
                    "microcode": rtcb.microcode,
                    "snp": rtcb.snp,
                    "tee": rtcb.tee,
                    "boot_loader": rtcb.bootloader,
                    "fmc": rtcb.fmc,
                },
            });
        }
    } else if let Some(m) = &e.message {
        obj["message"] = serde_json::json!(m);
    }
    if !meta.description.is_empty() {
        obj["description"] = serde_json::json!(meta.description);
    }
    let hint_str = effective_hint(&meta, &e.message);
    if !hint_str.is_empty() {
        obj["fix_hint"] = serde_json::json!(hint_str);
    }
    obj
}

/// Build a hierarchical JSON tree from a flat list of entries using their level field.
fn build_json_tree(entries: &[TestResultEntry]) -> Vec<serde_json::Value> {
    let mut root: Vec<serde_json::Value> = Vec::new();
    // Stack of (level, index into parent's children array)
    let mut stack: Vec<(usize, usize)> = Vec::new();

    for e in entries {
        let node = entry_to_json(e);

        // Pop stack until we find the parent level
        while let Some(&(lvl, _)) = stack.last() {
            if lvl >= e.level {
                stack.pop();
            } else {
                break;
            }
        }

        if stack.is_empty() {
            // Top-level node
            root.push(node);
            let idx = root.len() - 1;
            stack.push((e.level, idx));
        } else {
            // Find the parent node by traversing the tree
            let mut parent = &mut root[stack[0].1];
            for &(_, idx) in stack.iter().skip(1) {
                parent = &mut parent["children"][idx];
            }
            if parent.get("children").is_none() {
                parent["children"] = serde_json::json!([]);
            }
            parent["children"].as_array_mut().unwrap().push(node);
            let child_idx = parent["children"].as_array().unwrap().len() - 1;
            stack.push((e.level, child_idx));
        }
    }

    root
}

fn sw_to_json(v: &SoftwareVersion) -> serde_json::Value {
    let status = match v.status.as_str() {
        "ok" => "supported",
        "too_old" => "too_old",
        "missing" => "not_installed",
        "optional_missing" => "not_installed",
        _ => "unknown",
    };
    let mut obj = serde_json::json!({
        "component": v.name,
        "status": status,
    });
    if let Some(ver) = &v.version {
        obj["installed_version"] = serde_json::json!(ver);
    }
    // Extract min version from detail string (e.g. "kernel >= 6.11")
    if let Some(pos) = v.detail.find(">=") {
        let min = v.detail[pos + 2..].trim().to_string();
        obj["min_version"] = serde_json::json!(min);
    }
    obj
}

fn render_json(entries: &[TestResultEntry], sw_versions: &[SoftwareVersion], _passed: bool) {
    let mut pass_count = entries.iter().filter(|e| e.status == "PASS").count();
    let mut fail_count = entries.iter().filter(|e| e.status == "FAIL").count();
    let mut skip_count = entries.iter().filter(|e| e.status == "SKIP").count();
    for v in sw_versions {
        match v.status.as_str() {
            "ok" => pass_count += 1,
            "too_old" | "missing" => fail_count += 1,
            _ => skip_count += 1,
        }
    }

    let tests = build_json_tree(entries);
    let software: Vec<serde_json::Value> = sw_versions.iter().map(sw_to_json).collect();

    let output = serde_json::json!({
        "tests": tests,
        "software": software,
        "summary": {
            "passed": pass_count,
            "failed": fail_count,
            "skipped": skip_count,
            "total": pass_count + fail_count + skip_count,
        },
        "docs": "https://github.com/virtee/snphost/tree/main/docs/snphost-ok-reference.md",
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&output).unwrap_or_default()
    );
}

fn emit_result(res: &TestResult, level: usize, quiet: bool) {
    if !quiet {
        let meta = test_metadata(&res.name);
        let label = if meta.label.is_empty() {
            String::new()
        } else {
            format!(" {}", meta.label)
        };
        let msg = match &res.mesg {
            Some(m) => format!(": {}", m),
            None => "".to_string(),
        };
        let hint_str = effective_hint(&meta, &res.mesg);
        let hint = if res.stat == TestState::Fail && !hint_str.is_empty() {
            format!("\n{:width$}  ^ {} {}", "", "Hint:".blue(), hint_str, width = level + 10)
        } else {
            String::new()
        };
        println!(
            "[ {:^4} ] {:width$}- {}{}{}{}",
            format!("{}", res.stat),
            "",
            res.name,
            label,
            msg,
            hint,
            width = level
        )
    }
}

fn test_gen_not_included(test: &Test, level: usize, quiet: bool) {
    if !quiet {
        let tr_skip = TestResult {
            name: test.name.to_string(),
            stat: TestState::Skip,
            mesg: None,
        };

        println!(
            "[ {:^4} ] {:width$}- {}",
            format!("{}", tr_skip.stat),
            "",
            tr_skip.name,
            width = level
        );
        emit_skip(&test.sub, level + INDENT, quiet);
    }
}

fn emit_skip(tests: &[Test], level: usize, quiet: bool) {
    if !quiet {
        for t in tests {
            let tr_skip = TestResult {
                name: t.name.to_string(),
                stat: TestState::Skip,
                mesg: None,
            };

            println!(
                "[ {:^4} ] {:width$}- {}",
                format!("{}", tr_skip.stat),
                "",
                tr_skip.name,
                width = level
            );
            emit_skip(&t.sub, level + INDENT, quiet);
        }
    }
}

fn dev_sev_r() -> TestResult {
    let (stat, mesg) = match dev_sev_rw(fs::OpenOptions::new().read(true)) {
        Ok(_) => (TestState::Pass, None),
        Err(e) => (TestState::Fail, Some(format!("Not readable: {}", e))),
    };

    TestResult {
        name: "/dev/sev readable".to_string(),
        stat,
        mesg,
    }
}

fn dev_sev_w() -> TestResult {
    let (stat, mesg) = match dev_sev_rw(fs::OpenOptions::new().write(true)) {
        Ok(_) => (TestState::Pass, None),
        Err(e) => (TestState::Fail, Some(format!("Not writable: {}", e))),
    };

    TestResult {
        name: "/dev/sev writable".to_string(),
        stat,
        mesg,
    }
}

fn dev_sev_rw(file: &fs::OpenOptions) -> Result<()> {
    let path = "/dev/sev";

    match file.open(path) {
        Ok(_) => Ok(()),
        Err(e) => Err(anyhow::Error::new(Box::new(e))),
    }
}

fn has_kvm_support() -> TestResult {
    let path = "/dev/kvm";

    let (stat, mesg) = match File::open(path) {
        Ok(kvm) => {
            let api_version = unsafe { libc::ioctl(kvm.as_raw_fd(), 0xAE00, 0) };
            if api_version < 0 {
                (
                    TestState::Fail,
                    "Error - accessing KVM device node failed".to_string(),
                )
            } else {
                (TestState::Pass, format!("API version: {}", api_version))
            }
        }
        Err(e) => (TestState::Fail, format!("Error reading {}: ({})", path, e)),
    };

    TestResult {
        name: "KVM supported".to_string(),
        stat,
        mesg: Some(mesg),
    }
}

fn sev_enabled_in_kvm(gen: SevGeneration) -> TestResult {
    let path_loc = match gen {
        SevGeneration::Sev => "/sys/module/kvm_amd/parameters/sev",
        SevGeneration::Es => "/sys/module/kvm_amd/parameters/sev_es",
        SevGeneration::Snp => "/sys/module/kvm_amd/parameters/sev_snp",
    };
    let path = std::path::Path::new(path_loc);

    let (stat, mesg) = if path.exists() {
        match std::fs::read_to_string(path_loc) {
            Ok(result) => {
                if result.trim() == "1" || result.trim() == "Y" {
                    (TestState::Pass, None)
                } else {
                    (
                        TestState::Fail,
                        Some(format!(
                            "Error - contents read from {}: {}",
                            path_loc,
                            result.trim()
                        )),
                    )
                }
            }
            Err(e) => (
                TestState::Fail,
                Some(format!("Error - (unable to read {}): {}", path_loc, e,)),
            ),
        }
    } else {
        (
            TestState::Fail,
            Some(format!("Error - {} does not exist", path_loc)),
        )
    };

    TestResult {
        name: match gen {
            SevGeneration::Sev => "SEV enabled in KVM",
            SevGeneration::Es => "SEV-ES enabled in KVM",
            SevGeneration::Snp => "SEV-SNP enabled in KVM",
        }
        .to_string(),
        stat,
        mesg,
    }
}

fn memlock_rlimit() -> TestResult {
    let mut rlimit = MaybeUninit::uninit();
    let res = unsafe { libc::getrlimit(libc::RLIMIT_MEMLOCK, rlimit.as_mut_ptr()) };

    let (stat, mesg) = if res == 0 {
        let r = unsafe { rlimit.assume_init() };

        (
            TestState::Pass,
            format!("Soft: {} | Hard: {}", r.rlim_cur, r.rlim_max),
        )
    } else {
        (
            TestState::Fail,
            "Unable to retrieve memlock resource limits".to_string(),
        )
    };

    TestResult {
        name: "Memlock resource limit".to_string(),
        stat,
        mesg: Some(mesg),
    }
}

fn msr_bit_read(reg: u32, cpu: u16) -> Result<u64, anyhow::Error> {
    let mut msr = Msr::new(reg, cpu).context("Error Reading MSR")?;
    let raw_value = msr.read()?;
    Ok(raw_value)
}

fn sme_test() -> TestResult {
    let raw_value = match msr_bit_read(0xC0010010, 0) {
        Ok(raw) => raw,
        Err(e) => {
            return TestResult {
                name: "SME".to_string(),
                stat: TestState::Fail,
                mesg: format!("MSR read failed: {}", e).into(),
            }
        }
    };
    let (testres, mesg) = match (raw_value >> 23) & 1 {
        1 => (TestState::Pass, "Enabled in MSR"),
        0 => (TestState::Fail, "Disabled in MSR"),
        _ => unreachable!(),
    };
    TestResult {
        name: "SME".to_string(),
        stat: testres,
        mesg: Some(mesg.to_string()),
    }
}

fn snp_test() -> TestResult {
    let raw_value = match msr_bit_read(0xC0010010, 0) {
        Ok(raw) => raw,
        Err(e) => {
            return TestResult {
                name: "SNP".to_string(),
                stat: TestState::Fail,
                mesg: format!("MSR read failed: {}", e).into(),
            }
        }
    };
    let (testres, mesg) = match (raw_value >> 24) & 1 {
        1 => (TestState::Pass, "Enabled in MSR"),
        0 => (TestState::Fail, "Disabled in MSR"),
        _ => unreachable!(),
    };
    TestResult {
        name: "SNP".to_string(),
        stat: testres,
        mesg: Some(mesg.to_string()),
    }
}

fn get_rmp_address() -> TestResult {
    let rmp_base = match msr_bit_read(0xC0010132, 0) {
        Ok(raw) => raw,
        Err(e) => {
            return TestResult {
                name: "Reading RMP table".to_string(),
                stat: TestState::Fail,
                mesg: format!("Failed to read the desired MSR: {}", e).into(),
            }
        }
    };
    let rmp_end = match msr_bit_read(0xC0010133, 0) {
        Ok(raw) => raw,
        Err(e) => {
            return TestResult {
                name: "Reading RMP table".to_string(),
                stat: TestState::Fail,
                mesg: format!("Failed to read the desired MSR: {}", e).into(),
            }
        }
    };
    if rmp_base == 0 || rmp_end == 0 {
        TestResult {
            name: "Reading RMP table".to_string(),
            stat: TestState::Fail,
            mesg: Some("RMP table was not read successfully".into()),
        }
    } else {
        TestResult {
            name: "RMP table addresses".to_string(),
            stat: TestState::Pass,
            mesg: format!("0x{:x} - 0x{:x}", rmp_base, rmp_end).into(),
        }
    }
}

fn snp_ioctl(test: SnpStatusTest) -> TestResult {
    let status = match snp_platform_status() {
        Ok(stat) => stat,
        Err(e) => {
            return TestResult {
                name: test.to_string(),
                stat: TestState::Fail,
                mesg: Some(format!("Failed to get SNP Platform status {e}")),
            }
        }
    };

    match test {
        SnpStatusTest::Tcb => {
            if status.platform_tcb_version == status.reported_tcb_version {
                TestResult{
                            name: format!("{}", SnpStatusTest::Tcb),
                            stat: TestState::Pass,
                            mesg: format!("TCB versions match \n\n Platform TCB version: {} \n Reported TCB version: {}", 
                                        status.platform_tcb_version, status.reported_tcb_version).into()
                        }
            } else {
                TestResult {
                    name: format!("{}", SnpStatusTest::Tcb),
                    stat: TestState::Fail,
                    mesg: format!("The TCB versions did NOT match \n\n Platform TCB version: {} \n Reported TCB version: {}", 
                                    status.platform_tcb_version, status.reported_tcb_version).into(),
                }
            }
        }
        SnpStatusTest::Rmp => {
            if status.is_rmp_init.is_rmp_init() {
                TestResult {
                    name: format!("{}", SnpStatusTest::Rmp),
                    stat: TestState::Pass,
                    mesg: None,
                }
            } else {
                TestResult {
                    name: format!("{}", SnpStatusTest::Rmp),
                    stat: TestState::Fail,
                    mesg: None,
                }
            }
        }
        SnpStatusTest::AliasCheck => {
            if status.is_rmp_init.alias_check_complete() {
                TestResult {
                    name: format!("{}", SnpStatusTest::AliasCheck),
                    stat: TestState::Pass,
                    mesg: Some(
                        "Completed since last system update, no aliasing addresses".to_string(),
                    ),
                }
            } else {
                TestResult {
                    name: format!("{}", SnpStatusTest::AliasCheck),
                    stat: TestState::Fail,
                    mesg: None,
                }
            }
        }
        SnpStatusTest::Snp => {
            if status.state == 1 {
                TestResult {
                    name: format!("{}", SnpStatusTest::Snp),
                    stat: TestState::Pass,
                    mesg: None,
                }
            } else {
                TestResult {
                    name: format!("{}", SnpStatusTest::Snp),
                    stat: TestState::Fail,
                    mesg: None,
                }
            }
        }
    }
}

fn sev_ioctl(test: SevStatusTests) -> TestResult {
    let status = match sev_platform_status() {
        Ok(stat) => stat,
        Err(e) => {
            return TestResult {
                name: test.to_string(),
                stat: TestState::Fail,
                mesg: Some(format!("Failed to get SEV Platform Status {e}")),
            }
        }
    };
    match test {
        SevStatusTests::Sev => {
            if status.state == State::Working {
                TestResult {
                    name: format!("{}", SevStatusTests::Sev),
                    stat: TestState::Pass,
                    mesg: Some("Initialized, currently running a guest".to_string()),
                }
            } else if status.state == State::Initialized {
                TestResult {
                    name: format!("{}", SevStatusTests::Sev),
                    stat: TestState::Pass,
                    mesg: Some("Initialized, no guests running".to_string()),
                }
            } else {
                TestResult {
                    name: format!("{}", SevStatusTests::Sev),
                    stat: TestState::Fail,
                    mesg: Some("Uninitialized".to_string()),
                }
            }
        }

        SevStatusTests::Firmware => {
            if status.build.version == 0.into() {
                TestResult {
                    name: format!("{}", SevStatusTests::Firmware),
                    stat: TestState::Fail,
                    mesg: Some(format!(
                        "Invalid Firmware version: {}",
                        status.build.version
                    )),
                }
            } else if status.build.version.minor < 51 {
                TestResult {
                    name: format!("{}", SevStatusTests::Firmware),
                    stat: TestState::Fail,
                    mesg: format!(
                        "SEV firmware version needs to be at least 1.51, 
                            current firmware version: {}",
                        status.build.version
                    )
                    .into(),
                }
            } else {
                TestResult {
                    name: format!("{}", SevStatusTests::Firmware),
                    stat: TestState::Pass,
                    mesg: format!("{}", status.build.version).into(),
                }
            }
        }

        SevStatusTests::SevEs => {
            let res = match (status.flags.bits() >> 8) & 1 {
                1 => TestState::Pass,
                0 => TestState::Fail,
                _ => unreachable!(),
            };
            TestResult {
                name: format!("{}", SevStatusTests::SevEs),
                stat: res,
                mesg: None,
            }
        }
    }
}

// ── Software version checks ──────────────────────────────────────────

/// Result of a software version check
#[derive(Clone, serde::Serialize)]
struct SoftwareVersion {
    name: String,
    version: Option<String>,
    status: String, // "ok", "too_old", "missing", "optional_missing"
    detail: String,
}

fn check_kernel_version() -> SoftwareVersion {
    let name = "Linux kernel".to_string();
    match fs::read_to_string("/proc/sys/kernel/osrelease") {
        Ok(ver) => {
            let ver = ver.trim().to_string();
            let ok = parse_version_ge(&ver, 6, 11);
            SoftwareVersion {
                name,
                version: Some(ver),
                status: if ok { "ok" } else { "too_old" }.to_string(),
                detail: if ok {
                    "kernel >= 6.11".to_string()
                } else {
                    "Need kernel >= 6.11 for SEV-SNP host support".to_string()
                },
            }
        }
        Err(_) => SoftwareVersion {
            name,
            version: None,
            status: "missing".to_string(),
            detail: "Cannot read /proc/sys/kernel/osrelease".to_string(),
        },
    }
}

fn check_qemu_version() -> SoftwareVersion {
    let name = "QEMU".to_string();
    match Command::new("qemu-system-x86_64").arg("--version").output() {
        Ok(output) => {
            let out = String::from_utf8_lossy(&output.stdout);
            // e.g. "QEMU emulator version 8.2.2 ..."
            let ver = out
                .split_whitespace()
                .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()))
                .unwrap_or("unknown")
                .to_string();
            let ok = parse_version_ge(&ver, 6, 0);
            SoftwareVersion {
                name,
                version: Some(ver),
                status: if ok { "ok" } else { "too_old" }.to_string(),
                detail: if ok {
                    "qemu >= 6.0".to_string()
                } else {
                    "Need QEMU >= 6.0 for SEV-SNP support".to_string()
                },
            }
        }
        Err(_) => SoftwareVersion {
            name,
            version: None,
            status: "missing".to_string(),
            detail: "qemu-system-x86_64 not found".to_string(),
        },
    }
}

fn check_libvirt_version() -> SoftwareVersion {
    let name = "libvirt".to_string();
    match Command::new("virsh").arg("--version").output() {
        Ok(output) => {
            let ver = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let ok = parse_version_ge(&ver, 4, 5);
            SoftwareVersion {
                name,
                version: Some(ver),
                status: if ok { "ok" } else { "too_old" }.to_string(),
                detail: if ok {
                    "libvirt >= 4.5".to_string()
                } else {
                    "Need libvirt >= 4.5 for SEV support".to_string()
                },
            }
        }
        Err(_) => SoftwareVersion {
            name,
            version: None,
            status: "optional_missing".to_string(),
            detail: "virsh not found (optional, >= 4.5)".to_string(),
        },
    }
}

fn check_ovmf_version() -> SoftwareVersion {
    let name = "OVMF".to_string();
    let paths = [
        // Ubuntu/Debian
        "/usr/share/ovmf/OVMF.amdsev.fd",
        "/usr/share/OVMF/OVMF_CODE_4M.fd",
        // RHEL/Fedora (edk2)
        "/usr/share/edk2/ovmf/OVMF_CODE.fd",
        "/usr/share/edk2/x64/OVMF_CODE.fd",
        // SUSE
        "/usr/share/qemu/ovmf-x86_64-smm-ms-code.bin",
    ];
    let found = paths.iter().any(|p| std::path::Path::new(p).exists());
    if !found {
        return SoftwareVersion {
            name,
            version: None,
            status: "missing".to_string(),
            detail: "OVMF firmware not found".to_string(),
        };
    }
    // Try to get package version
    let ver = Command::new("dpkg-query")
        .args(["--showformat=${Version}", "--show", "ovmf"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .or_else(|| {
            Command::new("rpm")
                .args(["-q", "--qf", "%{VERSION}", "edk2-ovmf"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        });
    SoftwareVersion {
        name,
        version: ver,
        status: "ok".to_string(),
        detail: "OVMF firmware present".to_string(),
    }
}

fn check_sev_firmware_version() -> SoftwareVersion {
    let name = "SEV firmware".to_string();
    match sev_platform_status() {
        Ok(status) => {
            let ver = format!("{}", status.build.version);
            let ok = status.build.version.minor >= 51;
            SoftwareVersion {
                name,
                version: Some(ver),
                status: if ok { "ok" } else { "too_old" }.to_string(),
                detail: if ok {
                    "firmware >= 1.51".to_string()
                } else {
                    "Need firmware >= 1.51 for SNP".to_string()
                },
            }
        }
        Err(_) => SoftwareVersion {
            name,
            version: None,
            status: "missing".to_string(),
            detail: "Cannot query SEV platform status (need sudo?)".to_string(),
        },
    }
}

/// Parse a version string and check if it's >= major.minor
fn parse_version_ge(ver: &str, major: u32, minor: u32) -> bool {
    let parts: Vec<&str> = ver.split('.').collect();
    if parts.len() < 2 {
        return false;
    }
    let v_major = parts[0].parse::<u32>().unwrap_or(0);
    let v_minor = parts[1].parse::<u32>().unwrap_or(0);
    (v_major, v_minor) >= (major, minor)
}

/// Collect all software version checks
fn collect_software_versions() -> Vec<SoftwareVersion> {
    vec![
        check_kernel_version(),
        check_qemu_version(),
        check_libvirt_version(),
        check_ovmf_version(),
        check_sev_firmware_version(),
    ]
}

/// Print software versions for default mode
fn print_software_versions(versions: &[SoftwareVersion]) {
    for v in versions {
        let stat = match v.status.as_str() {
            "ok" => format!("{}", "PASS".green()),
            "too_old" => format!("{}", "FAIL".red()),
            "missing" => format!("{}", "SKIP".yellow()),
            "optional_missing" => format!("{}", "SKIP".yellow()),
            _ => format!("{}", "SKIP".yellow()),
        };
        let ver_str = v.version.as_deref().unwrap_or("not found");
        let detail = match v.status.as_str() {
            "missing" | "optional_missing" => "Not installed".to_string(),
            _ => ver_str.to_string(),
        };
        println!("[ {:^4} ] - {}: {} (Software)", stat, v.name, detail);
    }
}
