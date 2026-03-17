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

/// Arguments for the `ok` subcommand.
#[derive(Args)]
pub struct OkArgs {
    /// Show only failures and summary counts
    #[arg(long, conflicts_with_all = ["verbose", "json"])]
    pub short: bool,

    /// Show tests grouped by category with descriptions and recommended actions
    #[arg(long, conflicts_with_all = ["short", "json"])]
    pub verbose: bool,

    /// Output results as JSON
    #[arg(long, conflicts_with_all = ["short", "verbose"])]
    pub json: bool,
}

enum OutputFormat {
    Default,
    Short,
    Verbose,
    Json,
}

impl OkArgs {
    fn format(&self) -> OutputFormat {
        if self.short {
            OutputFormat::Short
        } else if self.verbose {
            OutputFormat::Verbose
        } else if self.json {
            OutputFormat::Json
        } else {
            OutputFormat::Default
        }
    }
}

// SEV generation-specific bitmasks.
const SEV_MASK: usize = 1;
const ES_MASK: usize = 1 << 1;
const SNP_MASK: usize = 1 << 2;

/// Category of test for grouping in verbose/JSON output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
            Self::CpuSupport => write!(f, "CPU Support"),
            Self::CpuInfo => write!(f, "CPU Info"),
            Self::BiosConfigured => write!(f, "BIOS Configured"),
            Self::PlatformInitialized => write!(f, "Platform Initialized"),
            Self::KvmConfig => write!(f, "KVM Config"),
            Self::Compliance => write!(f, "Compliance"),
        }
    }
}

struct Test {
    name: &'static str,
    gen_mask: usize,
    run: Box<TestFn>,
    sub: Vec<Test>,
    category: TestCategory,
    /// Parenthetical label appended to test name, e.g. "(CPU)", "(BIOS)".
    label: Option<&'static str>,
    /// Short description of what this test checks (for verbose mode).
    description: Option<&'static str>,
    /// Suggested fix when this test fails.
    fix_hint: Option<&'static str>,
}

struct TestResult {
    name: String,
    stat: TestState,
    mesg: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
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

/// A collected test result node, forming a tree that mirrors the test hierarchy.
struct TestResultNode {
    name: String,
    stat: TestState,
    mesg: Option<String>,
    level: usize,
    children: Vec<TestResultNode>,
    category: TestCategory,
    label: Option<String>,
    description: Option<String>,
    fix_hint: Option<String>,
}

fn collect_tests() -> Vec<Test> {
    use TestCategory::*;

    vec![
        Test {
            name: "AMD CPU",
            gen_mask: SEV_MASK,
            category: CpuSupport,
            label: Some("CPU"),
            description: Some("Checks CPU vendor string via CPUID is \"AuthenticAMD\""),
            fix_hint: Some("SEV-SNP requires an AMD processor"),
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
                    category: CpuSupport,
                    label: Some("CPU"),
                    description: Some("Verifies processor brand string contains \"EPYC\" (server-class CPU required)"),
                    fix_hint: Some("Need AMD EPYC 3rd Gen (Milan) or newer. Consumer Ryzen not supported"),
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
                    category: CpuSupport,
                    label: Some("CPU"),
                    description: Some("Checks CPUID 0x8000001F EAX bit 0 for SME hardware support"),
                    fix_hint: Some("Use AMD EPYC Naples or newer"),
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
                        category: BiosConfigured,
                        label: Some("BIOS"),
                        description: Some("Reads MSR 0xC0010010 (SYSCFG) bit 23 to verify SME enabled at system level"),
                        fix_hint: Some("Enable in BIOS: CBS > CPU Common > SMEE. Run: sudo modprobe msr"),
                        run: Box::new(sme_test),
                        sub: vec![],
                    }],
                },
                Test {
                    name: "Secure Encrypted Virtualization (SEV)",
                    gen_mask: SEV_MASK,
                    category: CpuSupport,
                    label: Some("CPU"),
                    description: Some("Checks CPUID 0x8000001F EAX bit 1 for SEV hardware support"),
                    fix_hint: Some("Ensure AMD EPYC processor with SEV support"),
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
                            category: BiosConfigured,
                            label: None,
                            description: Some("Queries /dev/sev PLATFORM_STATUS for firmware version (requires >= 1.51 for SNP)"),
                            fix_hint: Some("Run with sudo. Update BIOS to get firmware >= 1.51"),
                            run: Box::new(|| sev_ioctl(SevStatusTests::Firmware)),
                            sub: vec![],
                        },
                        Test {
                            name: "Encrypted State (SEV-ES)",
                            gen_mask: ES_MASK,
                            category: CpuSupport,
                            label: Some("CPU"),
                            description: Some("Checks CPUID 0x8000001F EAX bit 3 for SEV-ES hardware support"),
                            fix_hint: Some("Use AMD EPYC 2nd Gen (Rome) or newer"),
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
                                category: PlatformInitialized,
                                label: Some("FW Ready"),
                                description: Some("Queries SEV platform status flags bit 8 for SEV-ES initialization"),
                                fix_hint: Some("Run with sudo. Ensure kvm_amd loaded with sev-es=1"),
                                run: Box::new(|| sev_ioctl(SevStatusTests::SevEs)),
                                sub: vec![],
                            }],
                        },
                        Test {
                            name: "SEV initialized",
                            gen_mask: SNP_MASK,
                            category: PlatformInitialized,
                            label: Some("FW Ready"),
                            description: Some("Queries SEV platform status state field (must be Initialized or Working)"),
                            fix_hint: Some("Run with sudo. Load: sudo modprobe kvm_amd sev=1"),
                            run: Box::new(|| sev_ioctl(SevStatusTests::Sev)),
                            sub: vec![],
                        },
                        Test {
                            name: "Secure Nested Paging (SEV-SNP)",
                            gen_mask: SNP_MASK,
                            category: CpuSupport,
                            label: Some("CPU"),
                            description: Some("Checks CPUID 0x8000001F EAX bit 4 for SEV-SNP hardware support"),
                            fix_hint: Some("Requires AMD EPYC 3rd Gen (Milan) or newer"),
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
                                    category: CpuSupport,
                                    label: Some("CPU"),
                                    description: Some("Checks CPUID 0x8000001F EAX bit 5 for VMPL hardware support"),
                                    fix_hint: Some("Check for BIOS update or verify processor model"),
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
                                        category: CpuInfo,
                                        label: None,
                                        description: Some("Reads CPUID 0x8000001F EBX bits 15:12 for VMPL count (expected: 4)"),
                                        fix_hint: None,
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
                                    category: BiosConfigured,
                                    label: Some("BIOS"),
                                    description: Some("Reads MSR 0xC0010010 (SYSCFG) bit 24 to verify SNP enabled at system level"),
                                    fix_hint: Some("Enable in BIOS: CBS > CPU Common > SNP Memory Coverage. Run: sudo modprobe msr"),
                                    run: Box::new(snp_test),
                                    sub: vec![],
                                },
                                Test {
                                    name: "SNP initialized",
                                    gen_mask: SNP_MASK,
                                    category: PlatformInitialized,
                                    label: Some("FW Ready"),
                                    description: Some("Queries SNP_PLATFORM_STATUS state field = 1 (INIT state)"),
                                    fix_hint: Some("Run with sudo. Need kernel 6.11+. Load: sudo modprobe kvm_amd sev_snp=1"),
                                    run: Box::new(|| snp_ioctl(SnpStatusTest::Snp)),
                                    sub: vec![
                                        Test {
                                            name: "Read RMP tables",
                                            gen_mask: SNP_MASK,
                                            category: BiosConfigured,
                                            label: Some("BIOS"),
                                            description: Some("Reads MSRs 0xC0010132 and 0xC0010133 for RMP base/end addresses"),
                                            fix_hint: Some("Enable SNP Memory Coverage in BIOS. Run: sudo modprobe msr"),
                                            run: Box::new(get_rmp_address),
                                            sub: vec![],
                                        },
                                        Test {
                                            name: "RMP table initialized",
                                            gen_mask: SNP_MASK,
                                            category: PlatformInitialized,
                                            label: Some("FW Ready"),
                                            description: Some("Queries SNP platform status IS_RMP_INIT bit"),
                                            fix_hint: Some("Run with sudo. Need CONFIG_KVM_AMD_SEV=y. Reboot if firmware was updated"),
                                            run: Box::new(|| snp_ioctl(SnpStatusTest::Rmp)),
                                            sub: vec![],
                                        },
                                        Test {
                                            name: "Alias check",
                                            gen_mask: SNP_MASK,
                                            category: Compliance,
                                            label: None,
                                            description: Some("Queries SNP platform status ALIAS_CHECK_COMPLETE bit (CVE-2024-21944 mitigation)"),
                                            fix_hint: Some("Update SEV firmware and BIOS per AMD-SB-3015. Reboot required"),
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
                            category: CpuInfo,
                            label: None,
                            description: Some("Reads CPUID 0x8000001F EBX bits 11:6 for PA bit reduction value"),
                            fix_hint: None,
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
                            category: CpuInfo,
                            label: None,
                            description: Some("Reads CPUID 0x8000001F EBX bits 5:0 for encryption bit position in page tables"),
                            fix_hint: None,
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
                            category: CpuInfo,
                            label: None,
                            description: Some("Reads CPUID 0x8000001F ECX for maximum encrypted guest count"),
                            fix_hint: None,
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
                            category: CpuInfo,
                            label: None,
                            description: Some("Reads CPUID 0x8000001F EDX for minimum SEV-only ASID value"),
                            fix_hint: None,
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
                            category: PlatformInitialized,
                            label: None,
                            description: Some("Attempts to open /dev/sev device for reading"),
                            fix_hint: Some("Run with sudo. Load PSP driver: sudo modprobe ccp. Must run on baremetal"),
                            run: Box::new(dev_sev_r),
                            sub: vec![],
                        },
                        Test {
                            name: "/dev/sev writable",
                            gen_mask: SEV_MASK,
                            category: PlatformInitialized,
                            label: None,
                            description: Some("Attempts to open /dev/sev device for writing"),
                            fix_hint: Some("Run with sudo. Must run on baremetal. Check SELinux/AppArmor policies"),
                            run: Box::new(dev_sev_w),
                            sub: vec![],
                        },
                    ],
                },
                Test {
                    name: "Page flush MSR",
                    gen_mask: SEV_MASK,
                    category: CpuInfo,
                    label: None,
                    description: Some("Checks CPUID 0x8000001F EAX bit 2 for page flush MSR optimization support"),
                    fix_hint: None,
                    run: Box::new(|| {
                        let res = unsafe { x86_64::__cpuid(0x8000_001f) };

                        let msr_flag = if ((res.eax & 0x1) << 2) != 0 {
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
                            mesg: None,
                        }
                    }),
                    sub: vec![],
                },
            ],
        },
        Test {
            name: "KVM Support",
            gen_mask: SEV_MASK,
            category: KvmConfig,
            label: Some("KVM"),
            description: Some("Opens /dev/kvm and queries KVM API version via ioctl"),
            fix_hint: Some("Run with sudo. Load: sudo modprobe kvm && sudo modprobe kvm_amd. Enable SVM in BIOS"),
            run: Box::new(has_kvm_support),
            sub: vec![
                Test {
                    name: "SEV enabled in KVM",
                    gen_mask: SEV_MASK,
                    category: KvmConfig,
                    label: Some("KVM"),
                    description: Some("Reads /sys/module/kvm_amd/parameters/sev for \"1\" or \"Y\""),
                    fix_hint: Some("Set: options kvm_amd sev=1 in /etc/modprobe.d/kvm.conf"),
                    run: Box::new(|| sev_enabled_in_kvm(SevGeneration::Sev)),
                    sub: vec![],
                },
                Test {
                    name: "SEV-ES enabled in KVM",
                    gen_mask: ES_MASK,
                    category: KvmConfig,
                    label: Some("KVM"),
                    description: Some("Reads /sys/module/kvm_amd/parameters/sev_es for \"1\" or \"Y\""),
                    fix_hint: Some("Set: options kvm_amd sev-es=1 in /etc/modprobe.d/kvm.conf"),
                    run: Box::new(|| sev_enabled_in_kvm(SevGeneration::Es)),
                    sub: vec![],
                },
                Test {
                    name: "SEV-SNP enabled in KVM",
                    gen_mask: SNP_MASK,
                    category: KvmConfig,
                    label: Some("KVM"),
                    description: Some("Reads /sys/module/kvm_amd/parameters/sev_snp for \"1\" or \"Y\""),
                    fix_hint: Some("Set: options kvm_amd sev-snp=1 in /etc/modprobe.d/kvm.conf. Need kernel 6.11+"),
                    run: Box::new(|| sev_enabled_in_kvm(SevGeneration::Snp)),
                    sub: vec![],
                },
            ],
        },
        Test {
            name: "memlock limit",
            gen_mask: SEV_MASK,
            category: Compliance,
            label: None,
            description: Some("Reads RLIMIT_MEMLOCK soft and hard limits via getrlimit syscall"),
            fix_hint: Some("Set memlock unlimited: edit /etc/security/limits.conf or systemd LimitMEMLOCK=infinity"),
            run: Box::new(memlock_rlimit),
            sub: vec![],
        },
        Test {
            name: "Compare TCB values",
            gen_mask: SNP_MASK,
            category: Compliance,
            label: None,
            description: Some("Compares platform_tcb_version with reported_tcb_version from SNP_PLATFORM_STATUS"),
            fix_hint: Some("Run: sudo snphost commit (irreversible) or sudo snphost config set-reported-tcb"),
            run: Box::new(|| snp_ioctl(SnpStatusTest::Tcb)),
            sub: vec![],
        },
    ]
}

// ---------------------------------------------------------------------------
// Software version checks
// ---------------------------------------------------------------------------

struct SoftwareVersion {
    component: String,
    path: Option<String>,
    installed_version: Option<String>,
    min_version: Option<String>,
    status: SwVersionStatus,
}

#[derive(PartialEq, Eq)]
enum SwVersionStatus {
    Supported,
    NotInstalled,
    TooOld,
    PermissionDenied,
    Unknown,
}

/// Compare two dotted version strings (e.g. "6.14" >= "6.11").
/// Returns true if `have` >= `need`.
fn version_ge(have: &str, need: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.split('.')
            .filter_map(|p| {
                // Strip non-numeric suffixes (e.g. "37-generic" -> "37")
                let numeric: String = p.chars().take_while(|c| c.is_ascii_digit()).collect();
                numeric.parse().ok()
            })
            .collect()
    };
    let h = parse(have);
    let n = parse(need);
    for i in 0..h.len().max(n.len()) {
        let hv = h.get(i).copied().unwrap_or(0);
        let nv = n.get(i).copied().unwrap_or(0);
        if hv != nv {
            return hv > nv;
        }
    }
    true // equal
}

fn check_qemu_version() -> SoftwareVersion {
    let binary = "/usr/bin/qemu-system-x86_64";
    let path = if std::path::Path::new(binary).exists() {
        Some(binary.to_string())
    } else {
        None
    };

    let version = path.as_ref().and_then(|p| {
        Command::new(p)
            .arg("--version")
            .output()
            .ok()
            .and_then(|o| {
                let stdout = String::from_utf8_lossy(&o.stdout);
                // Parse: "QEMU emulator version X.Y.Z ..."
                stdout
                    .lines()
                    .next()
                    .and_then(|line| line.split("version ").nth(1))
                    .map(|v| v.split_whitespace().next().unwrap_or(v).to_string())
            })
    });

    let min = "6.0";
    let status = match (&path, &version) {
        (None, _) => SwVersionStatus::NotInstalled,
        (_, None) => SwVersionStatus::Unknown,
        (_, Some(v)) => {
            if version_ge(v, min) {
                SwVersionStatus::Supported
            } else {
                SwVersionStatus::TooOld
            }
        }
    };

    SoftwareVersion {
        component: "QEMU".to_string(),
        path,
        installed_version: version,
        min_version: Some(min.to_string()),
        status,
    }
}

fn check_libvirt_version() -> SoftwareVersion {
    let version = Command::new("virsh")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        });

    let min = "4.5";
    let status = match &version {
        None => SwVersionStatus::NotInstalled,
        Some(v) => {
            if version_ge(v, min) {
                SwVersionStatus::Supported
            } else {
                SwVersionStatus::TooOld
            }
        }
    };

    SoftwareVersion {
        component: "libvirt".to_string(),
        path: None,
        installed_version: version,
        min_version: Some(min.to_string()),
        status,
    }
}

fn check_ovmf_version() -> SoftwareVersion {
    let paths = [
        "/usr/share/ovmf/OVMF.amdsev.fd",
        "/usr/share/OVMF/OVMF_CODE.fd",
        "/usr/share/edk2/ovmf/OVMF_CODE.fd",
    ];
    let found_path = paths.iter().find(|p| std::path::Path::new(p).exists());

    let version = Command::new("dpkg-query")
        .args(["--showformat=${Version}", "--show", "ovmf"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .or_else(|| {
            Command::new("rpm")
                .args(["-q", "--qf", "%{VERSION}", "edk2-ovmf"])
                .output()
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                    } else {
                        None
                    }
                })
        });

    let status = match (&found_path, &version) {
        (None, _) => SwVersionStatus::NotInstalled,
        (Some(_), None) => SwVersionStatus::Unknown,
        (Some(_), Some(_)) => SwVersionStatus::Supported,
    };

    SoftwareVersion {
        component: "OVMF".to_string(),
        path: found_path.map(|p| p.to_string()),
        installed_version: version,
        min_version: None,
        status,
    }
}

fn check_kernel_version() -> SoftwareVersion {
    let version = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|s| s.trim().to_string())
        .ok();

    let min = "6.11";
    let status = match &version {
        None => SwVersionStatus::Unknown,
        Some(v) => {
            if version_ge(v, min) {
                SwVersionStatus::Supported
            } else {
                SwVersionStatus::TooOld
            }
        }
    };

    SoftwareVersion {
        component: "Kernel".to_string(),
        path: None,
        installed_version: version,
        min_version: Some(min.to_string()),
        status,
    }
}

fn check_sev_firmware_version() -> SoftwareVersion {
    let min = "1.51";
    match sev_platform_status() {
        Ok(status) => {
            let ver = format!("{}", status.build.version);
            let ok = status.build.version.minor >= 51;
            SoftwareVersion {
                component: "SEV Firmware".to_string(),
                path: Some("/dev/sev".to_string()),
                installed_version: Some(ver),
                min_version: Some(min.to_string()),
                status: if ok {
                    SwVersionStatus::Supported
                } else {
                    SwVersionStatus::TooOld
                },
            }
        }
        Err(e) => {
            let err_str = format!("{e}");
            let status = if err_str.contains("Permission denied") || err_str.contains("unable to open") {
                SwVersionStatus::PermissionDenied
            } else {
                SwVersionStatus::Unknown
            };
            SoftwareVersion {
                component: "SEV Firmware".to_string(),
                path: Some("/dev/sev".to_string()),
                installed_version: None,
                min_version: Some(min.to_string()),
                status,
            }
        }
    }
}

fn check_software_versions() -> Vec<SoftwareVersion> {
    vec![
        check_qemu_version(),
        check_libvirt_version(),
        check_ovmf_version(),
        check_kernel_version(),
        check_sev_firmware_version(),
    ]
}

const INDENT: usize = 2;

pub fn cmd(args: OkArgs, quiet: bool) -> Result<()> {
    let tests = collect_tests();
    let results = collect_results(&tests, 0, SEV_MASK | ES_MASK | SNP_MASK);
    let sw_versions = check_software_versions();

    if !quiet {
        match args.format() {
            OutputFormat::Default => {
                render_default(&results);
                println!();
                render_software_versions(&sw_versions);
            }
            OutputFormat::Short => {
                render_short(&results, &sw_versions);
            }
            OutputFormat::Verbose => {
                render_verbose(&results, &sw_versions);
            }
            OutputFormat::Json => {
                render_json(&results, &sw_versions);
            }
        }
    }

    if has_failures(&results) {
        Err(anyhow::anyhow!(
            "One or more tests in snphost ok reported a failure"
        ))
    } else {
        Ok(())
    }
}

/// Count pass/fail/skip across the entire result tree.
fn count_results(results: &[TestResultNode]) -> (usize, usize, usize) {
    let (mut pass, mut fail, mut skip) = (0, 0, 0);
    for r in results {
        match r.stat {
            TestState::Pass => pass += 1,
            TestState::Fail => fail += 1,
            TestState::Skip => skip += 1,
        }
        let (p, f, s) = count_results(&r.children);
        pass += p;
        fail += f;
        skip += s;
    }
    (pass, fail, skip)
}

/// Collect all failure nodes from the result tree.
fn collect_failures(results: &[TestResultNode]) -> Vec<&TestResultNode> {
    let mut failures = Vec::new();
    for r in results {
        if r.stat == TestState::Fail {
            failures.push(r);
        }
        failures.extend(collect_failures(&r.children));
    }
    failures
}

/// Short mode: failures-only compact output with summary counts.
fn render_short(results: &[TestResultNode], sw_versions: &[SoftwareVersion]) {
    let (pass, fail, skip) = count_results(results);
    let total = pass + fail + skip;
    println!("snphost ok: {}/{} passed, {} failed, {} skipped", pass, total, fail, skip);

    let failures = collect_failures(results);
    if !failures.is_empty() {
        println!("\nFAILURES:");
        for f in &failures {
            let msg = match &f.mesg {
                Some(m) => format!(": {}", m),
                None => String::new(),
            };
            let label = match &f.label {
                Some(l) => format!(" ({})", l),
                None => String::new(),
            };
            println!("  [{}] {}{}{}", "FAIL".red(), f.name, label, msg);
            if let Some(hint) = &f.fix_hint {
                println!("    Hint: {}", hint);
            }
        }
    }

    // Show software version issues
    let sw_issues: Vec<&SoftwareVersion> = sw_versions
        .iter()
        .filter(|v| v.status == SwVersionStatus::TooOld || v.status == SwVersionStatus::PermissionDenied)
        .collect();
    if !sw_issues.is_empty() {
        println!("\nSOFTWARE ISSUES:");
        for v in &sw_issues {
            let ver = v.installed_version.as_deref().unwrap_or("N/A");
            let min = v.min_version.as_deref().unwrap_or("N/A");
            match v.status {
                SwVersionStatus::TooOld => {
                    println!("  [{}] {}: {} (min: {})", "FAIL".red(), v.component, ver, min);
                }
                SwVersionStatus::PermissionDenied => {
                    println!("  [{}] {}: Permission denied (min: {})", "FAIL".red(), v.component, min);
                }
                _ => {}
            }
        }
    }
}

/// Flatten the result tree into a list of nodes for grouping by category.
fn flatten_results(results: &[TestResultNode]) -> Vec<&TestResultNode> {
    let mut flat = Vec::new();
    for r in results {
        flat.push(r);
        flat.extend(flatten_results(&r.children));
    }
    flat
}

/// Verbose mode: tests grouped by category with descriptions and recommended actions.
fn render_verbose(results: &[TestResultNode], sw_versions: &[SoftwareVersion]) {
    let flat = flatten_results(results);

    // Group by category (sorted by enum order since TestCategory derives Ord).
    let categories = [
        TestCategory::CpuSupport,
        TestCategory::CpuInfo,
        TestCategory::BiosConfigured,
        TestCategory::PlatformInitialized,
        TestCategory::KvmConfig,
        TestCategory::Compliance,
    ];

    for cat in &categories {
        let in_cat: Vec<&&TestResultNode> = flat.iter().filter(|n| n.category == *cat).collect();
        if in_cat.is_empty() {
            continue;
        }
        println!("{}:", cat);
        for node in &in_cat {
            let msg = match &node.mesg {
                Some(m) => format!(": {}", m),
                None => String::new(),
            };
            println!("  [ {:^4} ] {}{}", format!("{}", node.stat), node.name, msg);
            if let Some(desc) = &node.description {
                println!("           {}", desc);
            }
            if node.stat == TestState::Fail {
                if let Some(hint) = &node.fix_hint {
                    println!(
                        "           {} {}",
                        "Recommended action:".red(),
                        hint
                    );
                }
            }
        }
        println!();
    }

    // Software versions section
    println!("Installed Components:");
    for v in sw_versions {
        let stat_display = match v.status {
            SwVersionStatus::Supported => TestState::Pass,
            SwVersionStatus::TooOld => TestState::Fail,
            SwVersionStatus::PermissionDenied => TestState::Fail,
            SwVersionStatus::NotInstalled => TestState::Skip,
            SwVersionStatus::Unknown => TestState::Skip,
        };
        let ver = v.installed_version.as_deref().unwrap_or("N/A");
        let min_str = match &v.min_version {
            Some(m) => format!(" (min: {})", m),
            None => String::new(),
        };
        let detail = match v.status {
            SwVersionStatus::NotInstalled => format!("Not installed{}", min_str),
            SwVersionStatus::PermissionDenied => format!("Permission denied{}", min_str),
            _ => format!("{}{}", ver, min_str),
        };
        println!("  [ {:^4} ] {}: {}", format!("{}", stat_display), v.component, detail);
    }

    // DETECTED ISSUES summary
    let failures = collect_failures(results);
    if !failures.is_empty() {
        println!();
        println!("DETECTED ISSUES:");
        for f in &failures {
            let msg = match &f.mesg {
                Some(m) => format!(": {}", m),
                None => String::new(),
            };
            println!("  * {}{}", f.name, msg);
            if let Some(hint) = &f.fix_hint {
                println!("    -> {}", hint);
            }
        }
    }
}

/// JSON-serializable representation of a test result.
#[derive(serde::Serialize)]
struct JsonTestResult {
    name: String,
    #[serde(rename = "stat")]
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fix_hint: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<JsonTestResult>,
}

#[derive(serde::Serialize)]
struct JsonSoftwareVersion {
    component: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    installed_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_version: Option<String>,
    status: String,
}

#[derive(serde::Serialize)]
struct JsonSummary {
    passed: usize,
    failed: usize,
    skipped: usize,
    total: usize,
}

#[derive(serde::Serialize)]
struct JsonOutput {
    tests: Vec<JsonTestResult>,
    software: Vec<JsonSoftwareVersion>,
    summary: JsonSummary,
}

fn to_json_test(node: &TestResultNode) -> JsonTestResult {
    JsonTestResult {
        name: node.name.clone(),
        status: match node.stat {
            TestState::Pass => "pass".to_string(),
            TestState::Fail => "fail".to_string(),
            TestState::Skip => "skip".to_string(),
        },
        message: node.mesg.clone(),
        category: match node.category {
            TestCategory::CpuSupport => "cpu_support",
            TestCategory::CpuInfo => "cpu_info",
            TestCategory::BiosConfigured => "bios_configured",
            TestCategory::PlatformInitialized => "platform_initialized",
            TestCategory::KvmConfig => "kvm_config",
            TestCategory::Compliance => "compliance",
        }
        .to_string(),
        label: node.label.clone(),
        description: node.description.clone(),
        fix_hint: node.fix_hint.clone(),
        children: node.children.iter().map(to_json_test).collect(),
    }
}

fn to_json_sw(v: &SoftwareVersion) -> JsonSoftwareVersion {
    JsonSoftwareVersion {
        component: v.component.clone(),
        path: v.path.clone(),
        installed_version: v.installed_version.clone(),
        min_version: v.min_version.clone(),
        status: match v.status {
            SwVersionStatus::Supported => "supported",
            SwVersionStatus::NotInstalled => "not_installed",
            SwVersionStatus::TooOld => "too_old",
            SwVersionStatus::PermissionDenied => "permission_denied",
            SwVersionStatus::Unknown => "unknown",
        }
        .to_string(),
    }
}

/// JSON mode: machine-readable output with all metadata.
fn render_json(results: &[TestResultNode], sw_versions: &[SoftwareVersion]) {
    let (pass, fail, skip) = count_results(results);
    let output = JsonOutput {
        tests: results.iter().map(to_json_test).collect(),
        software: sw_versions.iter().map(to_json_sw).collect(),
        summary: JsonSummary {
            passed: pass,
            failed: fail,
            skipped: skip,
            total: pass + fail + skip,
        },
    };
    // JSON output goes to stdout; errors in has_failures() still go to stderr.
    println!("{}", serde_json::to_string_pretty(&output).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e)));
}

/// Run all tests and collect results into a tree, without printing.
fn collect_results(tests: &[Test], level: usize, mask: usize) -> Vec<TestResultNode> {
    let mut results = Vec::new();

    for t in tests {
        // Skip tests that aren't included in the specified generation.
        if (t.gen_mask & mask) != t.gen_mask {
            results.push(make_skip_node(t, level));
            continue;
        }

        let res = (t.run)();
        let children = match res.stat {
            TestState::Pass => collect_results(&t.sub, level + INDENT, mask),
            TestState::Fail => make_skip_tree(&t.sub, level + INDENT),
            TestState::Skip => unreachable!(),
        };

        results.push(TestResultNode {
            name: res.name,
            stat: res.stat,
            mesg: res.mesg,
            level,
            children,
            category: t.category,
            label: t.label.map(|s| s.to_string()),
            description: t.description.map(|s| s.to_string()),
            fix_hint: t.fix_hint.map(|s| s.to_string()),
        });
    }

    results
}

/// Create a skip node for a test not matching the generation mask.
fn make_skip_node(test: &Test, level: usize) -> TestResultNode {
    TestResultNode {
        name: test.name.to_string(),
        stat: TestState::Skip,
        mesg: None,
        level,
        children: make_skip_tree(&test.sub, level + INDENT),
        category: test.category,
        label: test.label.map(|s| s.to_string()),
        description: test.description.map(|s| s.to_string()),
        fix_hint: test.fix_hint.map(|s| s.to_string()),
    }
}

/// Recursively create skip nodes for all tests in a subtree.
fn make_skip_tree(tests: &[Test], level: usize) -> Vec<TestResultNode> {
    tests.iter().map(|t| make_skip_node(t, level)).collect()
}

/// Check if any node in the result tree is a failure.
fn has_failures(results: &[TestResultNode]) -> bool {
    for r in results {
        if r.stat == TestState::Fail {
            return true;
        }
        if has_failures(&r.children) {
            return true;
        }
    }
    false
}

/// Render results in the default format, with parenthetical labels added.
fn render_default(results: &[TestResultNode]) {
    for r in results {
        let msg = match &r.mesg {
            Some(m) => format!(": {}", m),
            None => String::new(),
        };
        let label = match &r.label {
            Some(l) => format!(" ({})", l),
            None => String::new(),
        };
        println!(
            "[ {:^4} ] {:width$}- {}{}{}",
            format!("{}", r.stat),
            "",
            r.name,
            label,
            msg,
            width = r.level
        );
        if r.stat == TestState::Fail {
            if let Some(hint) = &r.fix_hint {
                println!(
                    "         {:width$}  ^ {}",
                    "",
                    hint,
                    width = r.level
                );
            }
        }
        render_default(&r.children);
    }
}

/// Render software version checks in the default format.
fn render_software_versions(versions: &[SoftwareVersion]) {
    println!("Installed Components:");
    for v in versions {
        let stat_display = match v.status {
            SwVersionStatus::Supported => TestState::Pass,
            SwVersionStatus::TooOld => TestState::Fail,
            SwVersionStatus::PermissionDenied => TestState::Fail,
            SwVersionStatus::NotInstalled => TestState::Skip,
            SwVersionStatus::Unknown => TestState::Skip,
        };

        let path_str = match &v.path {
            Some(p) => format!(" [{}]", p),
            None => String::new(),
        };

        let min_str = match &v.min_version {
            Some(m) => format!(" (min: {})", m),
            None => String::new(),
        };

        let detail = match v.status {
            SwVersionStatus::NotInstalled => "Not installed".to_string(),
            SwVersionStatus::PermissionDenied => format!("Permission denied{}", min_str),
            _ => {
                let ver = v.installed_version.as_deref().unwrap_or("N/A");
                format!("{}{}", ver, min_str)
            }
        };

        println!(
            "[ {:^4} ] - {}{}: {}",
            format!("{}", stat_display),
            v.component,
            path_str,
            detail,
        );
    }
}

// ---------------------------------------------------------------------------
// Test implementation functions (unchanged)
// ---------------------------------------------------------------------------

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
