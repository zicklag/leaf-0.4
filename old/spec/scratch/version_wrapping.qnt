module version_wrapping {
  import model(VERSION_BITS = 3).*

  run OverflowDiffTest = all {
    assert(versionDiff(3, 4) == 1),
    assert(versionDiff(5, 4) == 6),
    assert(versionDiff(4, 4) == 7),
  }
}

module model {
  import basicSpells.* from "../lib/basicSpells"

  const VERSION_BITS: int
  pure val VERSION_MAX = 2^VERSION_BITS - 1

  type JobId = int

  var version: int
  var jobs: JobId -> int
  var jobAges: JobId -> int

  val Inv = all {
    jobs.keys().forall(jobId =>
      def jobVersion = jobs.get(jobId)
      def jobAge = jobAges.get(jobId)
      // The job version is only allowed to match the system version when the job has just been
      // created, otherwise the job needed to be removed before the system version wrapped around
      // and ended up equal to the job version again.
      jobVersion == version implies jobAge == 0
    )
  }

  action init = all {
    version' = 0,
    jobs' = Map(),
    jobAges' = Map(),
  }

  action step = any {
    updateVersionOrGarbageCollectJobs,
    createJob,
  }

  action updateVersionOrGarbageCollectJobs = 
    // If there is a job where we would overflow by updating the system version
    def overflowing = jobs.keys().filter(jobId =>
      def jobVersion = jobs.get(jobId)
      versionDiff(version, jobVersion) == 1
    )
    if (overflowing.size() > 0) 
      // We need to timeout all those jobs that would overflow
      all {
        version' = nextVersion(version),
        jobAges' = jobAges.keys().exclude(overflowing).map(jobId => (jobId, jobAges.get(jobId))).setToMap(),
        jobs' = jobs.keys().exclude(overflowing).map(jobId => (jobId, jobs.get(jobId))).setToMap(),
      }
    else
      all {
        // Increment the version
        version' = nextVersion(version),
        jobAges' = jobs.keys().map(jobId =>
          // Increment the age of the job
          (jobId, jobAges.get(jobId) + 1)
        ).setToMap(),
        jobs' = jobs,
      }

  action createJob =
    nondet jobId = 0.to(100).exclude(jobs.keys()).oneOf()
    all {
      version' = version,
      jobs' = jobs.put(jobId, version),
      jobAges' = jobAges.put(jobId, 0),
    }

  /// Increment the version number on the arbiter with a wrapping add.
  ///
  /// This doesn't make it _impossible_ for a version collision due to wrapping, but it
  /// would take a little over an hour with 1 million modifications per second, for the
  /// u32 to wrap, so it is a requirement that the wrapper call `timeout` on a job before
  /// it gets that old.
  /// 
  /// FIXME: Can we enforce that by forcing a timeout on old jobs?
  pure def nextVersion(a: int): int = (a + 1) % (2 ^ VERSION_BITS)

  /// Calculate the number of updates to the arbiter version before we end up colliding with the
  /// given job's version. We need to make sure we timeout the job before this happens to make sure
  /// that the job doesn't end up having the same version as the arbiter even though the arbiter
  /// has changed.
  pure def versionDiff(a: int, b: int): int = 
    // If the arbiter's version is less than the job's version, then it means that the arbiter has
    // hit the max version and started to wrap around
    if (a < b) b - a else

    // If the arbiter version is greater than the job version, then that means it hasn't wrapped
    // yet, and it can go up to the wrapping point, and then all the way back around up to the
    // arbiter version.
    if (a > b) VERSION_MAX - a + b else

    // If the versions are equal, we assume that they are the same, and we have the entire version
    // size before we start to wrap around.
    VERSION_MAX
}