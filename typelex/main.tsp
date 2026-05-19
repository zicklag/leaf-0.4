import "@typelex/emitter";
import "./externals.tsp";

/** Alias for a totally open union, requiring the record to be an object with a `$type` field. */
alias OpenUnion = unknown | never;

namespace town.muni.arbiter.defs {
  @inline union Member {
    MemberDid,
    MemberLocalSpace,
    MemberRemoteSpace,
  }

  @inline
  @maxLength(1024)
  scalar SpaceKey extends string;
  
  /**
   * The maximum depth to allow when doing remote resolutions.
   * 
   * Because spaces are allowed to have members resolved from remote spaces,
   * all of the arbiter endpoints have an optional resolution depth parameter
   * that can be used to limit how many chained remote calls can be resolved.
   * 
   * This applies both to queries and procedures. Queries may need to resolve
   * external members to complete the member list, and procedures may need
   * to resolve external members if the requesting member is not in the local
   * member list, to see if the member has access through a remote list.
   */
  @inline
  @minValue(0)
  scalar ResolverDepth extends integer;

  model MemberDid {
    @required did: did,
  }

  model MemberLocalSpace {
    @required spaceKey: SpaceKey,
  }

  model MemberRemoteSpace {
    @required arbiterDid: did,
    @required spaceKey: SpaceKey,
  }

  /** The specified DID already has an arbiter for it on this server. */
  model ErrArbiterAlreadyExists {}

  /** The requesting user is not allowed to make the request. */
  model ErrPermissionDenied {}

  /**
   * The config was changed by another request during the execution of this request.
   * 
   * The action should be retried if still applicable.
   * */
  model ErrRaceCondition {}

  /**
   * The config lexicon provided is not supported by this server. Different servers
   * may support different config lexicons.
   * */
  model ErrUnsupportedConfigLexicon {}

  /**
   * The config provided is invalid for some reason other than it being an unsupported
   * lexicon.
   */
  model ErrInvalidConfig {}

  /** The specified arbiter does not exist on this server. */
  model ErrArbiterNotExists {}

  /** The specified space already exists on this arbiter. */
  model ErrSpaceExists {}

  /** The specified space does not exist on this arbiter. */
  model ErrSpaceNotExists {}

  /** The specified member is not in the space. */
  model ErrMemberNotInSpace {}
}

namespace town.muni.arbiter.createArbiter {
  /**
   * Create a new arbiter.
   * 
   * The return type is an open union that will contain a `$type` key and may be
   * different depending on the server and/or the provided config `$type`.
   * */
  @procedure
  @errors(
    town.muni.arbiter.defs.ErrArbiterAlreadyExists,
    town.muni.arbiter.defs.ErrPermissionDenied,
    town.muni.arbiter.defs.ErrUnsupportedConfigLexicon,
    town.muni.arbiter.defs.ErrInvalidConfig,
  )
  op main(
    @required input: {
      /**
       * The DID of the arbiter.
       * 
       * Different servers may have different policies regarding who is allowed
       * to create an arbiter for what DIDs.
       * 
       * The DID will not be created by this procedure: it must be created
       * separately and have its #space_host service pointed to this server.
       */
      @required arbiterDid: did;
      /**
       * A configuration object for the arbiter.
       * 
       * Different servers will support different config lexicons. The NSID of
       * the config lexicon determines what kind of arbiter it is. Different
       * kinds of arbiters may have totally different mechanisms for determining
       * access.
       */
      @required config: OpenUnion
    },
  ): OpenUnion;
}

namespace town.muni.arbiter.deleteArbiter {
  /** Delete an arbiter from the server. */
  @procedure
  @errors(
    town.muni.arbiter.defs.ErrPermissionDenied,
  )
  op main(
    @required input: {
      // The DID of the arbiter to delete.
      @required arbiterDid: did,
      resolverDepth?: town.muni.arbiter.defs.ResolverDepth,
    }
  ): OpenUnion;
}

namespace town.muni.arbiter.setArbiterConfig {
  /** Set the config object for an arbiter. */
  @procedure
  @errors(
    town.muni.arbiter.defs.ErrPermissionDenied,
    town.muni.arbiter.defs.ErrInvalidConfig,
    town.muni.arbiter.defs.ErrUnsupportedConfigLexicon,
  )
  op main(
    @required input: {
      /** The DID of the arbiter to set the config for. */
      @required arbiterDid: did,
      /** 
       * The config object.
       * 
       * Different server's will support different config NSIDs, and most-likely servers
       * will not allow you to change the NSID after creation, even if they support
       * updating the config.
       */
      @required config: OpenUnion,
      resolverDepth?: town.muni.arbiter.defs.ResolverDepth,
    }
  ): OpenUnion;
}

namespace town.muni.arbiter.getArbiterConfig {
  /** Get the current config object for an arbiter. */
  @query
  @errors(town.muni.arbiter.defs.ErrPermissionDenied)
  op main(
    /** The DID of the arbiter to fetch the config from */
    @required arbiterDid: did;
    resolverDepth?: town.muni.arbiter.defs.ResolverDepth,
  ): OpenUnion;
}

namespace town.muni.arbiter.createSpace {
  /** Create a new space on an arbiter. */
  @procedure
  @errors(
    town.muni.arbiter.defs.ErrArbiterNotExists,
    town.muni.arbiter.defs.ErrSpaceExists,
    town.muni.arbiter.defs.ErrPermissionDenied,
    town.muni.arbiter.defs.ErrRaceCondition,
  )
  op main(
    @required input: {
      /** The DID of the arbiter to create the space in. */
      @required arbiterDid: did,
      /** The NSID of the space type. */
      @required spaceType: nsid,
      /** The key of the space to create. */
      @required spaceKey: town.muni.arbiter.defs.SpaceKey,
      /**
       * The space configuration.
       * 
       * Different servers will support different space config lexicons. Which
       * lexicons are supported will also likely depend on the arbiter's config type.
       */
      @required config: OpenUnion,
      resolverDepth?: town.muni.arbiter.defs.ResolverDepth,
    }
  ): OpenUnion;
}

namespace town.muni.arbiter.deleteSpace {
  /** Delete a space from the arbiter. */
  @procedure
  @errors(
    town.muni.arbiter.defs.ErrPermissionDenied,
    town.muni.arbiter.defs.ErrSpaceNotExists,
  )
  op main(
    @required input: {
      /** The DID of the arbiter to delete the space from. */
      @required arbiterDid: did,
      /** The space key of the space to delete. */
      @required spaceKey: town.muni.arbiter.defs.SpaceKey,
      resolverDepth?: town.muni.arbiter.defs.ResolverDepth,
    }
  ): OpenUnion;
}

namespace town.muni.arbiter.setSpaceConfig {
  /** Configure a space on the arbiter. */
  @procedure
  @errors(
    town.muni.arbiter.defs.ErrArbiterNotExists,
    town.muni.arbiter.defs.ErrSpaceNotExists,
    town.muni.arbiter.defs.ErrPermissionDenied,
    town.muni.arbiter.defs.ErrRaceCondition,
  )
  op main(
    @required input: {
      /** The DID of the arbiter that the space is in. */
      @required arbiterDid: did,
      /** The key of the space to configure. */
      @required spaceKey: town.muni.arbiter.defs.SpaceKey,
      /** The new type of the space. */
      @required spaceType: nsid,
      /** The new space configuration. */
      @required config: OpenUnion,
      resolverDepth?: town.muni.arbiter.defs.ResolverDepth,
    }
  ): OpenUnion;
}

namespace town.muni.arbiter.getSpaceConfig {
  /** Get the configuration of a space on the arbiter. */
  @query
  @errors(
    town.muni.arbiter.defs.ErrArbiterNotExists,
    town.muni.arbiter.defs.ErrSpaceNotExists,
    town.muni.arbiter.defs.ErrPermissionDenied,
  )
  op main(
    /** The DID of the arbiter that the space is in. */
    @required arbiterDid: did,
    /** The key of the space to configure. */
    @required spaceKey: town.muni.arbiter.defs.SpaceKey,
    resolverDepth?: town.muni.arbiter.defs.ResolverDepth,
  ): {
    /** The type of the space. */
    @required spaceType: nsid,
    /** The space configuration object. */
    @required config: OpenUnion,
  };
}

namespace town.muni.arbiter.getSpaceMembers {
  /**
   * Get the members that have been assigned to the space.
   * 
   * This is not a flat membership list, but a list of the direct members
   * of the space, including DIDs, local spaces, and remote spaces that have
   * been added as members, without doing any resolution of remote spaces.
   * */
  @query
  @errors(
    town.muni.arbiter.defs.ErrArbiterNotExists,
    town.muni.arbiter.defs.ErrSpaceNotExists,
    town.muni.arbiter.defs.ErrPermissionDenied,
  )
  op main(
    /** The DID of the arbiter that the space is in. */
    @required arbiterDid: did,
    /** The key of the space to configure. */
    @required spaceKey: town.muni.arbiter.defs.SpaceKey,
    resolverDepth?: town.muni.arbiter.defs.ResolverDepth,
  ): {
    /** The members of the space */
    @required members: {
      /** The member ID */
      @required member: town.muni.arbiter.defs.Member,
      /** The member's access config object. */
      @required access: OpenUnion,
    }[],
  };
}

namespace town.muni.arbiter.setSpaceMemberAccess {
  /**
   * Set the access of a member in a space.
   * 
   * This is also how you add a new member to the space.
   * */
  @procedure
  @errors(
    town.muni.arbiter.defs.ErrArbiterNotExists,
    town.muni.arbiter.defs.ErrSpaceNotExists,
    town.muni.arbiter.defs.ErrPermissionDenied,
    town.muni.arbiter.defs.ErrRaceCondition,
    town.muni.arbiter.defs.ErrMemberNotInSpace,
    town.muni.arbiter.defs.ErrInvalidConfig,
  )
  op main(
    @required input: {
      /** The DID of the arbiter that the space is in. */
      @required arbiterDid: did,
      /** The key of the space to configure. */
      @required spaceKey: town.muni.arbiter.defs.SpaceKey,
      /** The member to set the access of. */
      @required member: town.muni.arbiter.defs.Member,
      /** The updated access configuration for the member. */
      @required access: OpenUnion,
      resolverDepth?: town.muni.arbiter.defs.ResolverDepth,
    }
  ): OpenUnion;
}

namespace town.muni.arbiter.removeSpaceMember {
  /**
   * Remove a member's access from a space.
   * */
  @procedure
  @errors(
    town.muni.arbiter.defs.ErrArbiterNotExists,
    town.muni.arbiter.defs.ErrSpaceNotExists,
    town.muni.arbiter.defs.ErrPermissionDenied,
    town.muni.arbiter.defs.ErrRaceCondition,
    town.muni.arbiter.defs.ErrMemberNotInSpace,
  )
  op main(
    @required input: {
      /** The DID of the arbiter that the space is in. */
      @required arbiterDid: did,
      /** The key of the space to configure. */
      @required spaceKey: town.muni.arbiter.defs.SpaceKey,
      /** The member to set the access of. */
      @required member: town.muni.arbiter.defs.Member,
      /** The updated access configuration for the member. */
      @required access: OpenUnion,
      resolverDepth?: town.muni.arbiter.defs.ResolverDepth,
    }
  ): OpenUnion;
}

namespace town.muni.arbiter.resolveSpaceMembers {
  /**
   * Get the flattened, resolved list of members for a space.
   * 
   * This member list will be the same as what the server is expected to return
   * as the space host, but it will additionally contain a member config
   * object for each member.
   * 
   * The details of the member config object are dependent on the server implementation
   * and possibly the arbiter config type.
   * */
  @query
  @errors(
    town.muni.arbiter.defs.ErrArbiterNotExists,
    town.muni.arbiter.defs.ErrSpaceNotExists,
    town.muni.arbiter.defs.ErrPermissionDenied,
  )
  op main(
    /** The DID of the arbiter that the space is in. */
    @required arbiterDid: did,
    /** The key of the space to configure. */
    @required spaceKey: town.muni.arbiter.defs.SpaceKey,
    resolverDepth?: town.muni.arbiter.defs.ResolverDepth,
  ): {
    /** The members of the space */
    @required members: {
      /** The member DID. */
      @required did: did,
      /** The member's access config object. */
      @required access: OpenUnion,
    }[],
    /** 
     * Contains the list of remote spaces that could not be resolved.
     * 
     * If the hosts of remote spaces time out, then the server will reply with all
     * of the members that it was able to resolve and will list the spaces that
     * could not be resolved in `missingSpaces`.
     * */
    @required missingSpaces: {
      /** The space that could not be resolved. */
      @required space: town.muni.arbiter.defs.MemberRemoteSpace,
      /** The access object of the space that could not be resolved. */
      @required access: OpenUnion
    }[]
  };
}

namespace town.muni.arbiter.listSpaces {
  /**
   * Get the members that have been assigned to the space.
   * 
   * This is not a flat membership list, but a list of the direct members
   * of the space, including DIDs, local spaces, and remote spaces that have
   * been added as members, without doing any resolution of remote spaces.
   * */
  @query
  @errors(
    town.muni.arbiter.defs.ErrArbiterNotExists,
    town.muni.arbiter.defs.ErrSpaceNotExists,
    town.muni.arbiter.defs.ErrPermissionDenied,
  )
  op main(
    /** The DID of the arbiter that the space is in. */
    @required arbiterDid: did,
    resolverDepth?: town.muni.arbiter.defs.ResolverDepth,
  ): {
    /** The list of spaces on the arbiter that the requesting user has access to. */
    @required spaces: {
      /** The space's key. */
      @required spaceKey: town.muni.arbiter.defs.SpaceKey,
      /** The type of the space. */
      @required spaceType: nsid,
      /**
       * The space config.
       * 
       * This may not be returned if the server does not want to allow the requesting user
       * to read it.
       * */
      config?: OpenUnion,
    }[],
  };
}

