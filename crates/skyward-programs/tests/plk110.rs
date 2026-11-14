//! Real init/VINTF descriptions captured from PLK110, with empty markers
//! for binaries present on the phone (no proprietary binaries included).
use skyward_programs::{
    phone::{Partition, Phone, PARTITIONS},
    secure::{self, Step},
};
use std::{collections::HashMap, path::Path};

#[test]
fn plk110_discovers_keys_and_both_supplicant_services_from_stock_partitions() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plk110-stock");
    let partitions = PARTITIONS
        .iter()
        .map(|name| Partition {
            name: name.to_string(),
            mounted: format!("/{name}").into(),
            at: root.join(name),
        })
        .collect();
    let props: HashMap<String, String> = [
        ("ro.boot.hardware", "qcom"),
        ("ro.boot.product.vendor.sku", "canoe"),
        ("ro.boot.product.hardware.sku", "23"),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect();
    let phone = Phone::read(partitions, props).expect("all stock scripts and manifests must parse");
    let steps =
        secure::steps(&phone).expect("stock descriptions must resolve a complete key chain");
    let paths: Vec<_> = secure::programs(&steps)
        .into_iter()
        .map(|p| p.service.path.as_str())
        .collect();
    println!("key programs: {paths:#?}");
    let order: Vec<_> = steps
        .iter()
        .map(|step| match step {
            Step::Start(p) => p.service.path.rsplit('/').next().unwrap(),
            Step::ServeElements => "OMAPI",
        })
        .collect();
    assert_eq!(
        order,
        [
            "qseecomd",
            "hlosminkdaemon",
            "android.hardware.security.onekeymint-service-qti",
            "android.hardware.gatekeeper-rust-service-qti",
            "android.hardware.secure_element-service.qti",
            "OMAPI",
            "android.hardware.weaver-service.nxp",
            "android.hardware.boot-service.qti"
        ]
    );
    let offered = phone.told_of();
    println!(
        "supplicant descriptions: {:?}",
        offered
            .iter()
            .filter(|n| n.contains("supplicant"))
            .collect::<Vec<_>>()
    );
    assert!(offered
        .iter()
        .any(|n| n == "android.hardware.wifi.supplicant.ISupplicant/default"));
    for vendor in [
        "vendor.qti.hardware.wifi.supplicant.ISupplicantVendor/default",
        "vendor.oplus.hardware.wifi.supplicant.IOplusSupplicant/default",
    ] {
        assert!(
            offered.iter().any(|n| n == vendor),
            "{vendor} must be declared too"
        );
    }
    let written = skyward_programs::description::offering(&offered).unwrap();
    let read_back: Vec<_> = skyward_programs::description::read(&written)
        .unwrap()
        .into_iter()
        .map(|d| d.name())
        .collect();
    assert_eq!(
        read_back, offered,
        "the recovery manifest preserves every stock AIDL declaration"
    );
    let key_programs = secure::programs(&steps);
    // DT_NEEDED libEseUtils.so exists only in /vendor/lib64/hw on PLK110.
    // Its ordinary vendor/odm directories cannot satisfy that dependency.
    let element = key_programs.iter().find(|p| p.service.path.ends_with("secure_element-service.qti")).unwrap();
    assert_eq!(element.service.libraries.first().map(String::as_str), Some("/system/lib64"),
        "keep recovery Binder/runtime libraries first");
    for path in ["/vendor/lib64/hw", "/odm/lib64/hw"] {
        assert!(element.service.libraries.iter().any(|p| p == path),
            "the eSE must be able to load its dependency from {path}");
    }
    let weaver = key_programs
        .iter()
        .find(|p| p.service.path.ends_with("weaver-service.nxp"))
        .unwrap();
    assert_eq!(weaver.service.user.as_deref(), Some("vendor_nxp_weaver"));
    assert!(weaver
        .service
        .capabilities
        .iter()
        .any(|c| c == "WAKE_ALARM"));
    assert!(weaver.anew);
    let lists: Vec<_> = [
        "vendor/etc/passwd",
        "vendor/etc/group",
        "odm/etc/passwd",
        "odm/etc/group",
    ]
    .iter()
    .map(|p| root.join(p).to_string_lossy().into_owned())
    .collect();
    let known =
        skyward_sys::ueventd::accounts(&lists.iter().map(String::as_str).collect::<Vec<_>>());
    for p in key_programs {
        for name in p
            .service
            .user
            .iter()
            .chain(&p.service.groups)
            .chain(p.sockets.iter().flat_map(|s| s.user.iter().chain(&s.group)))
        {
            assert!(
                skyward_sys::ueventd::account(&known, name).is_some(),
                "unknown account {name} for {}",
                p.service.path
            );
        }
        for name in p.service.ready_service.iter().chain(&p.service.registers) {
            assert!(
                offered.contains(name),
                "{} registers undeclared {name}",
                p.service.path
            );
        }
    }
}
