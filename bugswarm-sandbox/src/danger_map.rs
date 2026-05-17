use serde::{Deserialize, Serialize};
use anyhow::Context;

/// A single danger map entry as received from the CPG daemon in JSON form.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DangerMapEntryJson {
    pub address: String,
    pub danger_score: f32,
}

/// Full danger map response from the CPG daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DangerMapResponse {
    pub num_entries: usize,
    pub sink_count: usize,
    pub decay_factor: f32,
    pub entries: Vec<DangerMapEntryJson>,
    pub timestamp: String,
}

impl DangerMapResponse {
    /// Convert JSON-formatted entries into the internal [`DangerMap`].
    pub fn to_danger_map(&self) -> DangerMap {
        let pairs: Vec<(u64, f32)> = self
            .entries
            .iter()
            .map(|e| {
                let addr_str = e
                    .address
                    .trim_start_matches("0x")
                    .trim_start_matches("0X");
                let addr = u64::from_str_radix(addr_str, 16).unwrap_or(0);
                (addr, e.danger_score)
            })
            .collect();
        DangerMap::from_pairs(pairs)
    }
}

/// A sorted map of (address, danger_score) pairs used for taint-guided
/// fuzzing prioritization.
#[derive(Debug, Clone)]
pub struct DangerMap {
    pub pairs: Vec<(u64, f32)>,
}

impl DangerMap {
    pub fn new() -> Self {
        Self {
            pairs: Vec::new(),
        }
    }

    pub fn from_pairs(mut pairs: Vec<(u64, f32)>) -> Self {
        pairs.sort_by_key(|&(addr, _)| addr);
        pairs.dedup_by(|a, b| a.0 == b.0);
        Self { pairs }
    }

    pub fn sort_pairs(&mut self) {
        self.pairs.sort_by_key(|(addr, _)| *addr);
    }

    /// Binary search with floor semantics:
    /// - Exact match: return the score.
    /// - Between entries: return the score of the nearest lower address.
    /// - Below all entries: return 0.0.
    pub fn lookup(&self, address: u64) -> f32 {
        if self.pairs.is_empty() {
            return 0.0;
        }
        match self.pairs.binary_search_by_key(&address, |&(a, _)| a) {
            Ok(idx) => self.pairs[idx].1,
            Err(idx) => {
                if idx == 0 {
                    0.0
                } else {
                    self.pairs[idx - 1].1
                }
            }
        }
    }

    /// Normalize all scores to [0.0, 1.0] by dividing by the maximum score.
    pub fn normalize(&mut self) {
        if self.pairs.is_empty() {
            return;
        }

        // Filter out NaN and INF values before computing max
        let max_score = self.pairs.iter()
            .map(|&(_, s)| s)
            .filter(|s| s.is_finite())
            .fold(0.0_f32, f32::max);

        if max_score <= 0.0 || !max_score.is_finite() {
            return;
        }

        for (_, score) in &mut self.pairs {
            if score.is_finite() {
                *score /= max_score;
            } else {
                *score = 0.0;
            }
        }
    }

    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// Serialize as binary: 8-byte header (u32 count + u32 reserved),
    /// then N entries of (u64 address + f32 score + 4 bytes padding = 16 bytes each).
    pub fn to_bytes(&self) -> Vec<u8> {
        let count = self.pairs.len() as u32;
        let mut buf = Vec::with_capacity(8 + count as usize * 16);
        buf.extend_from_slice(&count.to_le_bytes());
        buf.extend_from_slice(&[0u8; 4]); // reserved
        for &(addr, score) in &self.pairs {
            buf.extend_from_slice(&addr.to_le_bytes());
            buf.extend_from_slice(&score.to_le_bytes());
            buf.extend_from_slice(&[0u8; 4]); // padding
        }
        buf
    }

    /// Deserialize from binary format.
    pub fn from_bytes(data: &[u8]) -> anyhow::Result<Self> {
        if data.len() < 8 {
            anyhow::bail!("data too short for header");
        }
        let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let expected_len = 8 + count * 16;
        if data.len() < expected_len {
            anyhow::bail!(
                "data too short: expected {} bytes, got {}",
                expected_len,
                data.len()
            );
        }
        if data.len() > expected_len {
            log::warn!("from_bytes: {} trailing bytes ignored", data.len() - expected_len);
        }
        let mut pairs = Vec::with_capacity(count);
        for i in 0..count {
            let offset = 8 + i * 16;
            let addr = u64::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ]);
            let score = f32::from_le_bytes([
                data[offset + 8],
                data[offset + 9],
                data[offset + 10],
                data[offset + 11],
            ]);
            pairs.push((addr, score));
        }
        Ok(Self { pairs })
    }
}

impl Default for DangerMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration controlling how taint-guided danger scores are blended into
/// the fuzzer's power schedule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DangerConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_taint_weight")]
    pub taint_weight: f32,
    #[serde(default = "default_coverage_weight")]
    pub coverage_weight: f32,
    #[serde(default = "default_decay_factor")]
    pub decay_factor: f32,
}

fn default_true() -> bool {
    true
}
fn default_taint_weight() -> f32 {
    0.7
}
fn default_coverage_weight() -> f32 {
    0.3
}
fn default_decay_factor() -> f32 {
    0.7
}

impl Default for DangerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            taint_weight: 0.7,
            coverage_weight: 0.3,
            decay_factor: 0.7,
        }
    }
}

impl DangerConfig {
    /// Validate configuration values.
    pub fn validate(&self) -> Result<(), String> {
        if self.taint_weight < 0.0 {
            return Err(format!("taint_weight must be >= 0.0, got {}", self.taint_weight));
        }
        if self.coverage_weight < 0.0 {
            return Err(format!("coverage_weight must be >= 0.0, got {}", self.coverage_weight));
        }
        if self.decay_factor < 0.0 {
            return Err(format!("decay_factor must be >= 0.0, got {}", self.decay_factor));
        }
        Ok(())
    }

    /// Create with validation.
    pub fn new_validated(taint_weight: f32, coverage_weight: f32, decay_factor: f32) -> Result<Self, String> {
        let config = Self { enabled: true, taint_weight, coverage_weight, decay_factor };
        config.validate()?;
        Ok(config)
    }

    /// Blend coverage rarity and danger score into a power-schedule value
    /// clamped to [0.0, 1.0].
    pub fn compute_power_schedule(&self, coverage_rarity: f32, danger_score: f32) -> f32 {
        let cov = coverage_rarity.max(0.0).min(1.0);
        let dng = danger_score.max(0.0).min(1.0);
        let score = cov * self.coverage_weight + dng * self.taint_weight;
        score.max(0.0).min(1.0)
    }
}

/// Convenience free function: compute combined power score from danger and coverage.
pub fn compute_power_score(danger_score: f32, coverage_rarity: f32, config: &DangerConfig) -> f32 {
    config.compute_power_schedule(coverage_rarity, danger_score)
}

// ---------------------------------------------------------------------------
// Shared memory transport
// ---------------------------------------------------------------------------

/// Create a POSIX shared memory segment, return its file descriptor.
pub fn shm_create(name: &str, size: usize) -> anyhow::Result<i32> {
    let c_name = std::ffi::CString::new(name)
        .with_context(|| format!("invalid SHM name: {}", name))?;
    // SAFETY: shm_open is called with a valid NUL-terminated CString pointer,
    // O_RDWR|O_CREAT|O_EXCL for exclusive creation, and mode 0o600.
    // The CString lives for the duration of this call.
    let fd = unsafe {
        libc::shm_open(
            c_name.as_ptr(),
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
            0o600 as libc::mode_t,
        )
    };
    if fd < 0 {
        anyhow::bail!("shm_open({}) failed: {}", name, std::io::Error::last_os_error());
    }
    // SAFETY: ftruncate is called with a valid fd from shm_open (checked >= 0 above),
    // and the size argument matches the requested segment size.
    if unsafe { libc::ftruncate(fd, size as libc::off_t) } < 0 {
        let err = std::io::Error::last_os_error();
        // SAFETY: close and shm_unlink are called with a valid fd and valid CString
        // to clean up resources on ftruncate failure.
        unsafe {
            libc::close(fd);
            libc::shm_unlink(c_name.as_ptr());
        }
        anyhow::bail!("ftruncate({}, {}) failed: {}", name, size, err);
    }
    Ok(fd)
}

/// Mmap a shared memory segment read-write, returning a raw pointer.
pub fn shm_map(fd: i32, size: usize) -> anyhow::Result<*mut u8> {
    // SAFETY: mmap is called with a valid file descriptor from shm_open,
    // size matching the segment size from fstat, PROT_READ|PROT_WRITE only,
    // MAP_SHARED for read/write access, and offset 0.
    // The returned pointer is raw and the caller must ensure cleanup (munmap + close).
    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd,
            0,
        )
    };
    if ptr == libc::MAP_FAILED {
        anyhow::bail!("mmap failed: {}", std::io::Error::last_os_error());
    }
    Ok(ptr as *mut u8)
}

/// Unlink a POSIX shared memory segment by name.
pub fn shm_unlink(name: &str) -> anyhow::Result<()> {
    let c_name = std::ffi::CString::new(name)
        .with_context(|| format!("invalid SHM name: {}", name))?;
    // SAFETY: shm_unlink is called with a valid NUL-terminated CString pointer.
    // The CString lives for the duration of this call.
    let rc = unsafe { libc::shm_unlink(c_name.as_ptr()) };
    if rc < 0 {
        let err = std::io::Error::last_os_error();
        if err.kind() != std::io::ErrorKind::NotFound {
            anyhow::bail!("shm_unlink({}) failed: {}", name, err);
        }
    }
    Ok(())
}

/// Serialize a DangerMap and write it to a POSIX shared memory segment.
pub fn danger_map_to_shm(map: &DangerMap, shm_name: &str) -> anyhow::Result<()> {
    let bytes = map.to_bytes();
    let fd = shm_create(shm_name, bytes.len())?;
    let ptr = shm_map(fd, bytes.len())?;
    // SAFETY: copy_nonoverlapping copies exactly bytes.len() bytes from a valid
    // Vec<u8> to the mmap'd writable region of the same size. After the copy,
    // munmap and close clean up the mapping and file descriptor.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        libc::munmap(ptr as *mut libc::c_void, bytes.len());
        libc::close(fd);
    }
    Ok(())
}

/// Open a shared memory segment, deserialize a DangerMap from it, and unlink.
pub fn danger_map_from_shm(shm_name: &str) -> anyhow::Result<DangerMap> {
    let c_name = std::ffi::CString::new(shm_name)
        .with_context(|| format!("invalid SHM name: {}", shm_name))?;
    // SAFETY: shm_open is called with a valid NUL-terminated CString, O_RDONLY
    // for read-only access, and mode 0. The CString is valid for this call.
    let fd = unsafe { libc::shm_open(c_name.as_ptr(), libc::O_RDONLY, 0) };
    if fd < 0 {
        anyhow::bail!(
            "shm_open({}) for read failed: {}",
            shm_name,
            std::io::Error::last_os_error()
        );
    }
    // SAFETY: std::mem::zeroed() is safe for libc::stat which is a POD struct;
    // all-zero bit pattern is a valid initialized value.
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: fstat is called with a valid fd from shm_open and a mutable
    // reference to a zeroed stat struct that will be filled by the kernel.
    if unsafe { libc::fstat(fd, &mut stat) } < 0 {
        let err = std::io::Error::last_os_error();
        // SAFETY: close on a valid fd to clean up after fstat failure.
        unsafe {
            libc::close(fd);
        }
        anyhow::bail!("fstat({}) failed: {}", shm_name, err);
    }
    let size = stat.st_size as usize;
    // SAFETY: mmap is called with a valid fd from shm_open, size from fstat
    // which matches the segment size, PROT_READ only, MAP_SHARED for read-only
    // access, and offset 0. The returned pointer is immediately converted to a
    // slice via from_raw_parts with the same size.
    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ,
            libc::MAP_SHARED,
            fd,
            0,
        )
    };
    if ptr == libc::MAP_FAILED {
        let err = std::io::Error::last_os_error();
        // SAFETY: close on a valid fd to clean up after mmap failure.
        unsafe {
            libc::close(fd);
        }
        anyhow::bail!("mmap({}) failed: {}", shm_name, err);
    }
    // SAFETY: from_raw_parts constructs an &[u8] over the mmap'd memory region
    // with the exact same size as returned by fstat. The memory is valid for the
    // lifetime of this function (munmap at end).
    let slice = unsafe { std::slice::from_raw_parts(ptr as *const u8, size) };
    let map = DangerMap::from_bytes(slice)?;
    // SAFETY: munmap unmaps the shared memory region, and close releases the fd.
    // Both pointer and fd are valid from the mmap/shm_open calls above.
    unsafe {
        libc::munmap(ptr as *mut libc::c_void, size);
        libc::close(fd);
    }
    shm_unlink(shm_name)?;
    Ok(map)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // DangerMap core
    // ------------------------------------------------------------------

    #[test]
    fn test_empty_map_returns_zero() {
        let map = DangerMap::new();
        assert_eq!(map.lookup(0x1000), 0.0);
    }

    #[test]
    fn test_exact_match() {
        let map = DangerMap::from_pairs(vec![(0x1000, 0.5), (0x2000, 0.8)]);
        assert_eq!(map.lookup(0x1000), 0.5);
        assert_eq!(map.lookup(0x2000), 0.8);
    }

    #[test]
    fn test_floor_semantics() {
        let map = DangerMap::from_pairs(vec![(0x1000, 0.3), (0x3000, 0.9)]);
        assert_eq!(map.lookup(0x2000), 0.3);
    }

    #[test]
    fn test_below_all() {
        let map = DangerMap::from_pairs(vec![(0x5000, 0.7)]);
        assert_eq!(map.lookup(0x1000), 0.0);
    }

    #[test]
    fn test_normalize() {
        let mut map = DangerMap::from_pairs(vec![(0x1000, 2.0), (0x2000, 4.0), (0x3000, 4.0)]);
        map.normalize();
        assert!((map.lookup(0x1000) - 0.5).abs() < 1e-6);
        assert!((map.lookup(0x2000) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_empty() {
        let mut map = DangerMap::new();
        map.normalize();
        assert_eq!(map.len(), 0);
    }

    #[test]
    fn test_len() {
        let map = DangerMap::from_pairs(vec![(0x1000, 0.1), (0x2000, 0.2)]);
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn test_danger_config_defaults() {
        let cfg = DangerConfig::default();
        assert!(cfg.enabled);
        assert!((cfg.taint_weight - 0.7).abs() < 1e-6);
        assert!((cfg.coverage_weight - 0.3).abs() < 1e-6);
        assert!((cfg.decay_factor - 0.7).abs() < 1e-6);
    }

    #[test]
    fn test_compute_power_schedule() {
        let cfg = DangerConfig::default();
        let score = cfg.compute_power_schedule(0.5, 0.8);
        let expected = 0.5 * 0.3 + 0.8 * 0.7;
        assert!((score - expected).abs() < 1e-6);
    }

    #[test]
    fn test_compute_power_schedule_clamped() {
        let cfg = DangerConfig::default();
        let score = cfg.compute_power_schedule(1.5, 2.0);
        assert!(score <= 1.0);
    }

    // ------------------------------------------------------------------
    // Binary serialization
    // ------------------------------------------------------------------

    #[test]
    fn test_danger_map_serialization_roundtrip() {
        let mut dm = DangerMap::new();
        dm.pairs.push((0x1000, 0.5));
        dm.pairs.push((0x2000, 0.8));
        dm.pairs.push((0x3000, 1.0));

        let bytes = dm.to_bytes();
        let dm2 = DangerMap::from_bytes(&bytes).unwrap();

        assert_eq!(dm2.len(), 3);
        assert!((dm2.lookup(0x1000) - 0.5).abs() < 0.001);
        assert!((dm2.lookup(0x2000) - 0.8).abs() < 0.001);
        assert!((dm2.lookup(0x3000) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_danger_map_from_bytes_empty() {
        let bytes = DangerMap::new().to_bytes();
        let dm = DangerMap::from_bytes(&bytes).unwrap();
        assert_eq!(dm.len(), 0);
    }

    #[test]
    fn test_danger_map_from_bytes_truncated() {
        let bytes = vec![0u8; 3]; // too short for header
        assert!(DangerMap::from_bytes(&bytes).is_err());
    }

    #[test]
    fn test_danger_map_lookup_no_exact_match() {
        let mut dm = DangerMap::new();
        dm.pairs.push((0x1000, 0.3));
        dm.pairs.push((0x2000, 0.7));
        dm.pairs.push((0x3000, 1.0));
        dm.sort_pairs(); // ensure sorted
        assert!((dm.lookup(0x1500) - 0.3).abs() < 0.001); // floor to 0x1000
        assert!((dm.lookup(0x2500) - 0.7).abs() < 0.001); // floor to 0x2000
        assert!((dm.lookup(0x4000) - 1.0).abs() < 0.001); // floor to 0x3000
        assert!((dm.lookup(0x0500) - 0.0).abs() < 0.001); // below all
    }

    // ------------------------------------------------------------------
    // Power schedule
    // ------------------------------------------------------------------

    #[test]
    fn test_power_schedule_basic() {
        let config = DangerConfig::default();
        // High danger: 70% danger + 30% coverage
        let score = config.compute_power_schedule(0.5, 0.9);
        assert!((score - (0.5 * 0.3 + 0.9 * 0.7)).abs() < 0.001);

        // No danger: 30% coverage only
        let score = config.compute_power_schedule(0.8, 0.0);
        assert!((score - (0.8 * 0.3)).abs() < 0.001);

        // Max: 1.0
        let score = config.compute_power_schedule(1.0, 1.0);
        assert!((score - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_power_schedule_custom_weights() {
        let config = DangerConfig {
            taint_weight: 1.0,
            coverage_weight: 0.0,
            ..Default::default()
        };
        let score = config.compute_power_schedule(0.5, 0.9);
        assert!((score - 0.9).abs() < 0.001); // danger only
    }

    #[test]
    fn test_power_schedule_zero_both() {
        let config = DangerConfig::default();
        let score = config.compute_power_schedule(0.0, 0.0);
        assert!((score - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_normalize_danger_map() {
        let mut dm = DangerMap::new();
        dm.pairs.push((0x1000, 5.0));
        dm.pairs.push((0x2000, 2.5));
        dm.pairs.push((0x3000, 7.5));
        dm.sort_pairs();
        dm.normalize();
        assert!((dm.lookup(0x3000) - 1.0).abs() < 0.001);
        assert!((dm.lookup(0x1000) - (5.0 / 7.5)).abs() < 0.001);
        assert!((dm.lookup(0x2000) - (2.5 / 7.5)).abs() < 0.001);
    }

    // ------------------------------------------------------------------
    // Shared memory
    // ------------------------------------------------------------------

    #[test]
    fn test_shm_create_and_unlink() {
        let name = "/bugswarm_test_shm_21a";
        let _ = shm_unlink(name); // cleanup first
        let fd = shm_create(name, 4096).unwrap();
        assert!(fd >= 0);
        let ptr = shm_map(fd, 4096).unwrap();
        assert!(!ptr.is_null());
        // SAFETY: munmap with a valid non-null pointer from shm_map and the
        // correct size 4096 bytes. Test invariant: ptr is not null (asserted above).
        unsafe {
            libc::munmap(ptr as *mut libc::c_void, 4096);
        }
        // SAFETY: close with a valid file descriptor from shm_create (checked >= 0 above).
        unsafe {
            libc::close(fd);
        }
        shm_unlink(name).unwrap();
    }

    #[test]
    fn test_danger_map_shm_roundtrip() {
        let name = "/bugswarm_danger_shm_test";
        let _ = shm_unlink(name);

        let mut dm = DangerMap::new();
        dm.pairs.push((0x1000, 0.25));
        dm.pairs.push((0x2000, 0.75));
        dm.sort_pairs();

        danger_map_to_shm(&dm, name).unwrap();
        let dm2 = danger_map_from_shm(name).unwrap();

        assert_eq!(dm2.len(), 2);
        assert!((dm2.lookup(0x1000) - 0.25).abs() < 0.001);
        assert!((dm2.lookup(0x2000) - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_shm_unlink_nonexistent() {
        // Should not panic, return Ok or ignore error
        let _ = shm_unlink("/nonexistent_shm_21a_test");
    }

    // ------------------------------------------------------------------
    // Dedup / validate / NaN safety / from_bytes trailing garbage
    // ------------------------------------------------------------------

    #[test]
    fn test_dedup_duplicate_addresses() {
        let dm = DangerMap::from_pairs(vec![
            (0x1000, 0.1), (0x2000, 0.5), (0x1000, 0.9)
        ]);
        assert_eq!(dm.len(), 2, "Duplicate addresses should be deduplicated");
        let score = dm.lookup(0x1000);
        assert!((score - 0.1).abs() < 0.001 || (score - 0.9).abs() < 0.001,
            "Should keep one of the duplicate entries");
    }

    #[test]
    fn test_danger_config_validate() {
        let valid = DangerConfig::new_validated(0.7, 0.3, 0.7);
        assert!(valid.is_ok());

        let neg = DangerConfig::new_validated(-0.1, 0.3, 0.7);
        assert!(neg.is_err());

        let neg2 = DangerConfig::new_validated(0.7, -0.3, 0.7);
        assert!(neg2.is_err());
    }

    #[test]
    fn test_normalize_with_nan() {
        let mut dm = DangerMap::from_pairs(vec![
            (0x1000, 0.5), (0x2000, f32::NAN), (0x3000, 1.0)
        ]);
        dm.normalize();
        assert!((dm.lookup(0x1000) - 0.5).abs() < 0.01, "Finite values should normalize");
        assert_eq!(dm.lookup(0x2000), 0.0, "NaN should become 0 after normalize");
        assert!((dm.lookup(0x3000) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_normalize_with_infinity() {
        let mut dm = DangerMap::from_pairs(vec![
            (0x1000, 2.0), (0x2000, f32::INFINITY), (0x3000, 1.0)
        ]);
        dm.normalize();
        assert_eq!(dm.lookup(0x2000), 0.0, "INF should become 0 after normalize");
        // max finite is 2.0, so 0x3000 -> 1.0/2.0 = 0.5
        assert!((dm.lookup(0x3000) - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_from_bytes_trailing_garbage() {
        let dm = DangerMap::from_pairs(vec![(0x1000, 0.5)]);
        let mut bytes = dm.to_bytes();
        bytes.extend_from_slice(&[0xFF; 10]);
        let result = DangerMap::from_bytes(&bytes);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().lookup(0x1000), 0.5);
    }
}
