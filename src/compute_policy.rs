//! Conservative routing for approximate compute. No device initialization here.
//! Profiles must include all wall-clock costs at the worker boundary.
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precision {
    ExactCad,
    ApproximateValidated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Cpu,
    Auto,
    PreferGpu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Workload {
    pub kernel_revision: u64,
    pub adapter_generation: u64,
    pub elements: usize,
    pub upload_bytes: u64,
    pub download_bytes: u64,
    pub cold: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Profile {
    pub workload: Workload,
    pub samples: u32,
    pub cpu_wall: Duration,
    pub gpu_wall: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resources {
    pub available: bool,
    pub required_bytes: u64,
    pub budget_bytes: u64,
    pub largest_binding_bytes: u64,
    pub max_binding_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    CpuForced,
    CpuPrecision,
    CpuEmpty,
    CpuUnavailable,
    CpuResourceLimit,
    CpuUnmeasured,
    CpuNotFaster,
    GpuPreferred,
    GpuMeasured,
}

/// A profile is specific to the kernel, adapter generation, transfer sizes,
/// element count and cold/warm state. The caller invalidates it when any
/// configuration or data-dependent performance assumption changes.
/// Resource estimates must include all live and in-flight allocations.
pub fn choose(
    mode: Mode,
    precision: Precision,
    workload: Workload,
    resources: Resources,
    profile: Option<Profile>,
) -> Route {
    if mode == Mode::Cpu {
        return Route::CpuForced;
    }
    if precision == Precision::ExactCad {
        return Route::CpuPrecision;
    }
    if workload.elements == 0 {
        return Route::CpuEmpty;
    }
    if !resources.available {
        return Route::CpuUnavailable;
    }
    if resources.required_bytes > resources.budget_bytes
        || resources.largest_binding_bytes > resources.max_binding_bytes
        || resources.largest_binding_bytes > resources.required_bytes
        || resources.required_bytes == 0
    {
        return Route::CpuResourceLimit;
    }
    if mode == Mode::PreferGpu {
        return Route::GpuPreferred;
    }
    let Some(profile) = profile else {
        return Route::CpuUnmeasured;
    };
    if profile.workload != workload
        || profile.samples < 5
        || profile.cpu_wall.is_zero()
        || profile.gpu_wall.is_zero()
    {
        return Route::CpuUnmeasured;
    }
    // Require >=20% wall-time reduction; integer arithmetic avoids overflow
    // from multiplying a Duration and avoids floating-point rounding.
    if profile.gpu_wall.as_nanos() * 5 <= profile.cpu_wall.as_nanos() * 4 {
        Route::GpuMeasured
    } else {
        Route::CpuNotFaster
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Workload = Workload {
        kernel_revision: 1,
        adapter_generation: 2,
        elements: 100_000,
        upload_bytes: 1_600_000,
        download_bytes: 1_600_000,
        cold: false,
    };
    const RES: Resources = Resources {
        available: true,
        required_bytes: 4_800_000,
        budget_bytes: 8_000_000,
        largest_binding_bytes: 1_600_000,
        max_binding_bytes: 2_000_000,
    };

    fn profile(cpu: u64, gpu: u64) -> Profile {
        Profile {
            workload: WORK,
            samples: 5,
            cpu_wall: Duration::from_micros(cpu),
            gpu_wall: Duration::from_micros(gpu),
        }
    }

    fn auto(p: Option<Profile>) -> Route {
        choose(Mode::Auto, Precision::ApproximateValidated, WORK, RES, p)
    }

    #[test]
    fn auto_requires_measured_margin() {
        assert_eq!(auto(None), Route::CpuUnmeasured);
        assert_eq!(auto(Some(profile(100, 80))), Route::GpuMeasured);
        assert_eq!(auto(Some(profile(100, 81))), Route::CpuNotFaster);
        assert_eq!(auto(Some(profile(100, 200))), Route::CpuNotFaster);
        assert_eq!(auto(Some(profile(100, 0))), Route::CpuUnmeasured);
    }

    #[test]
    fn stale_profiles_are_rejected() {
        let mut p = profile(100, 10);
        p.workload.cold = true;
        assert_eq!(auto(Some(p)), Route::CpuUnmeasured);
        p.workload = WORK;
        p.workload.adapter_generation += 1;
        assert_eq!(auto(Some(p)), Route::CpuUnmeasured);
        p.workload = WORK;
        p.samples = 4;
        assert_eq!(auto(Some(p)), Route::CpuUnmeasured);
        p.samples = 5;
        p.workload.upload_bytes += 16;
        assert_eq!(auto(Some(p)), Route::CpuUnmeasured);
    }

    #[test]
    fn explicit_gpu_cannot_bypass_safety() {
        assert_eq!(
            choose(Mode::PreferGpu, Precision::ExactCad, WORK, RES, None),
            Route::CpuPrecision
        );
        let mut resources = RES;
        resources.available = false;
        assert_eq!(
            choose(
                Mode::PreferGpu,
                Precision::ApproximateValidated,
                WORK,
                resources,
                None
            ),
            Route::CpuUnavailable
        );
        resources = RES;
        resources.budget_bytes = 1;
        assert_eq!(
            choose(
                Mode::PreferGpu,
                Precision::ApproximateValidated,
                WORK,
                resources,
                None
            ),
            Route::CpuResourceLimit
        );
        assert_eq!(
            choose(Mode::PreferGpu, Precision::ApproximateValidated, WORK, RES, None),
            Route::GpuPreferred
        );
    }

    #[test]
    fn extreme_durations_do_not_overflow() {
        let p = Profile {
            workload: WORK,
            samples: 5,
            cpu_wall: Duration::MAX,
            gpu_wall: Duration::MAX,
        };
        assert_eq!(auto(Some(p)), Route::CpuNotFaster);
    }

    #[test]
    fn empty_and_forced_cpu_do_not_dispatch() {
        let empty = Workload {
            elements: 0,
            ..WORK
        };
        assert_eq!(
            choose(Mode::PreferGpu, Precision::ApproximateValidated, empty, RES, None),
            Route::CpuEmpty
        );
        assert_eq!(
            choose(Mode::Cpu, Precision::ApproximateValidated, WORK, RES, None),
            Route::CpuForced
        );
    }
}
