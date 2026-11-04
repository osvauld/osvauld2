use super::*;

// Every input is built by the same double arithmetic on both sides, so Lua and Rust see the same bits.
const SWEEP: u32 = 4000;
fn x(i: u32) -> f64 {
    -50.0 + i as f64 * 0.0251
}
fn y(i: u32) -> f64 {
    0.37 + i as f64 * 0.00113
}

const SWEEP_LUA: &str = r#"
local out, n = {}, 0
local function put(v) n += 1; out[n] = v end
for i = 0, 3999 do
	local x, y = -50.0 + i * 0.0251, 0.37 + i * 0.00113
	local a = math.abs(x)
	put(math.sin(x)); put(math.cos(x)); put(math.tan(x)); put(math.atan(x))
	put(math.asin(x / 50)); put(math.acos(x / 50)); put(math.atan2(x, y))
	put(math.sinh(x / 8)); put(math.cosh(x / 8)); put(math.tanh(x / 8))
	put(math.exp(x / 4)); put(math.log(a + y)); put(math.log10(a + y)); put(math.log(a + y, 3))
	put(math.log(a + y, 2)); put(math.pow(a, y)); put(math.lerp(x, y, 0.3)); put(math.map(x, -50, 50, y, 9))
end
return out
"#;

fn expected() -> Vec<f64> {
    let mut out = Vec::new();
    for i in 0..SWEEP {
        let (x, y) = (x(i), y(i));
        let a = x.abs();
        out.extend([
            libm::sin(x),
            libm::cos(x),
            libm::tan(x),
            libm::atan(x),
            libm::asin(x / 50.0),
            libm::acos(x / 50.0),
            libm::atan2(x, y),
            libm::sinh(x / 8.0),
            libm::cosh(x / 8.0),
            libm::tanh(x / 8.0),
            libm::exp(x / 4.0),
            libm::log(a + y),
            libm::log10(a + y),
            libm::log(a + y) / libm::log(3.0),
            libm::log2(a + y),
            libm::pow(a, y),
            x + (y - x) * 0.3,
            y + (x - -50.0) * (9.0 - y) / (50.0 - -50.0),
        ]);
    }
    out
}

fn sweep() -> Vec<f64> {
    let (vm, _budget) = sandboxed_vm().unwrap();
    let t: Table = vm.load(SWEEP_LUA).eval().unwrap();
    t.sequence_values::<f64>().map(Result::unwrap).collect()
}

#[test]
fn math_is_the_same_on_every_device() {
    let (got, want) = (sweep(), expected());
    assert_eq!(got.len(), want.len());
    let bad: Vec<usize> = (0..got.len()).filter(|&i| got[i].to_bits() != want[i].to_bits()).collect();
    assert!(bad.is_empty(), "{} of {} differ, first at {}: {} vs {}", bad.len(), got.len(), bad[0], got[bad[0]], want[bad[0]]);
}

// The Lua sweep's own bits, recorded once. The test above compares two uses of the same local
// libm; this one also fails where a binding went missing and the platform's answer came back.
#[test]
fn lua_sweep_is_pinned() {
    let digest = sweep().iter().fold(0xcbf29ce484222325u64, |h, v| (h ^ v.to_bits()).wrapping_mul(0x100000001b3));
    assert_eq!(digest, SWEEP_DIGEST, "{digest:#x}");
}
// Recorded on x86_64 Linux 2026-11-04; arm64 must agree.
const SWEEP_DIGEST: u64 = 0x514dc4e63ea9b139;

// libm's own answers, recorded once: a libm upgrade that changes a bit is a protocol change for
// every match in flight between two versions.
#[test]
fn libm_answers_are_pinned() {
    let got = [libm::sin(1.0), libm::exp(1.0), libm::atan2(1.0, 2.0), libm::pow(2.0, 0.3)].map(f64::to_bits);
    assert_eq!(got, RECORDED, "{got:#x?}");
}
// Recorded on aarch64 macOS 2026-11-04; x86_64 Linux must agree.
const RECORDED: [u64; 4] = [0x3feaed548f090cee, 0x4005bf0a8b14576a, 0x3fddac670561bb4f, 0x3ff3b2c47bff8329];

#[test]
fn noise_is_refused() {
    let (vm, _budget) = sandboxed_vm().unwrap();
    let err = vm.load("return math.noise(0.5, 0.5)").eval::<f64>().unwrap_err().to_string();
    assert!(err.contains("math.noise"), "{err}");
}

#[test]
fn math_cannot_be_put_back() {
    let (vm, _budget) = sandboxed_vm().unwrap();
    assert!(vm.load("math.sin = function() return 0 end").exec().is_err());
}

// `a % b` is `a - floor(a / b) * b` in the VM's C++; a compiler left to fuse that into one rounding
// does so on arm64 and cannot on x86_64. `.cargo/config.toml` turns fusing off.
#[test]
fn float_mod_is_not_fused() {
    let (vm, _budget) = sandboxed_vm().unwrap();
    let lua = "local out = {} for i = 0, 3999 do out[i + 1] = (i * 1.37 + 0.1) % (0.7 + i * 0.013) end return out";
    let t: Table = vm.load(lua).eval().unwrap();
    let got: Vec<f64> = t.sequence_values::<f64>().map(Result::unwrap).collect();
    let (mut fusing_differs, mut bad) = (0, 0);
    for i in 0..4000 {
        let (a, b) = (i as f64 * 1.37 + 0.1, 0.7 + i as f64 * 0.013);
        let plain = a - (a / b).floor() * b;
        if (-(a / b).floor()).mul_add(b, a) != plain {
            fusing_differs += 1;
        }
        if got[i as usize].to_bits() != plain.to_bits() {
            bad += 1;
        }
    }
    assert!(fusing_differs > 0, "the sweep never reaches where fusing changes the answer");
    assert_eq!(bad, 0, "{bad} of 4000 came out fused");
}

#[test]
fn the_mod_probe_sees_fusing() {
    let (a, b) = (3.0f64 * 1.37 + 0.1, 0.7 + 3.0 * 0.013);
    assert_ne!((-(a / b).floor()).mul_add(b, a), a - (a / b).floor() * b, "probe input does not discriminate");
    let (vm, _budget) = sandboxed_vm().unwrap();
    assert!(vm.app_data_ref::<crate::dmath::Agrees>().unwrap().0, "this build's Luau fuses float ops");
}

#[test]
fn vector_is_refused() {
    let (vm, _budget) = sandboxed_vm().unwrap();
    let err = vm.load("return vector.create(1, 2, 3)").exec().unwrap_err().to_string();
    assert!(err.contains("vector is not the same"), "{err}");
}
