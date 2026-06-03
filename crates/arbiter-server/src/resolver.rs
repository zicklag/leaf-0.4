use std::sync::{Arc, LazyLock};

use atproto_identity::{
    resolve::{HickoryDnsResolver, InnerIdentityResolver, SharedIdentityResolver},
    traits::IdentityResolver,
};

use crate::CONFIG;

pub static RESOLVER: LazyLock<Arc<dyn IdentityResolver>> = LazyLock::new(|| {
    // Identity resolver
    let resolver_client = reqwest::Client::new();
    let dns_resolver = HickoryDnsResolver::create_resolver(&[]);
    let identity_resolver = SharedIdentityResolver(Arc::new(InnerIdentityResolver {
        dns_resolver: Arc::new(dns_resolver),
        http_client: resolver_client,
        plc_hostname: CONFIG.plc_directory_url.clone(),
    }));
    Arc::new(identity_resolver)
});
