module utils {
  import basicSpells.* from "./basicSpells"
  export basicSpells.*

  /// Helper that returns the first option if it's some, or the other option.
  pure def someOrOther(o: Option[a], other: Option[a]): Option[a] =
    match (o) {
      | Some(d) => Some(d)
      | None => other
    }

  /// Return the Some value of an option or else a default value
  pure def orElse(o: Option[a], default: a): a =
    match (o) {
      | Some(d) => d
      | None => default
    }
}