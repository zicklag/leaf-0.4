module utils {
  import basicSpells.* from "./basicSpells"
  export basicSpells.*

  // Helper that returns the first option if it's some, or the other option.
  pure def someOrOther(o: Option[a], other: Option[a]): Option[a] =
    match (o) {
      | Some(d) => Some(d)
      | None => other
    }
}