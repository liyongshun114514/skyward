# PLK110 stock description fixture

Captured 2026-11-14 from the user's OnePlus 15 (PLK110), vendor SKU `canoe`, hardware SKU `23`, hardware `qcom`.

The `system_ext`, `vendor`, and `odm` directories contain the stock `etc/init`, `etc/vintf`, account lists, and build properties. Absolute manifest symlinks were dereferenced when capturing. Paths under `bin` are **empty presence markers**, not executable vendor binaries, so `Phone::carries()` sees exactly the files present on the phone. No user data, passwords, key blobs or credentials were collected.

Keep this outside `tests/described`: the older Xiaomi-oriented matrix assumes all hardware SKU variants resolve identically and that init always uses the same QCV naming script. The independent `tests/plk110.rs` supplies this phone's real SKU values and tests its own key chain, accounts/capabilities, OMAPI dependency, and all three Android/QTI/OPlus supplicant declarations.

Regression: `vendor/etc/init/android.hardware.contexthub-service.qmi.rc` begins with `//move to odm to control project`. Stock init ignores invalid lines outside any section and continues. Skyward v16 incorrectly failed the entire Phone read here, preventing both key discovery and vendor WLAN declaration publication.
