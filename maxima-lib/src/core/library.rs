use anyhow::Result;

use super::{
    auth::storage::LockedAuthStorage,
    concurrency::execute_batch_concurrent,
    locale::Locale,
    service_layer::{
        ServiceGameProductType, ServiceGetPreloadedOwnedGamesRequest,
        ServiceGetPreloadedOwnedGamesRequestBuilder, ServiceLayerClient, ServicePlatform,
        ServiceStorefront, ServiceUser, ServiceUserGameProduct,
        SERVICE_REQUEST_GETPRELOADEDOWNEDGAMES,
    },
};

pub struct GameLibrary {
    service_layer: ServiceLayerClient,
    library: Vec<ServiceUserGameProduct>,
}

impl GameLibrary {
    pub fn new(auth: LockedAuthStorage) -> Self {
        Self {
            service_layer: ServiceLayerClient::new(auth),
            library: Vec::new(),
        }
    }

    pub async fn owned_games(&self) {
        self.request_page_concurrent(Locale::EnUs, 1).await.unwrap();
    }

    async fn request_page_concurrent(&self, locale: Locale, page: u32) -> Result<()> {
        let requests = vec![
            GameLibrary::library_request(&locale, ServiceGameProductType::DigitalFullGame, true, page)?,
            GameLibrary::library_request(&locale, ServiceGameProductType::DigitalFullGame, false, page)?,
            GameLibrary::library_request(&locale, ServiceGameProductType::DigitalExtraContent, false, page)?,
        ];

        let requests_and_clients = requests
            .iter()
            .map(|x| (x.clone(), self.service_layer.clone()))
            .collect();

        let responses: Vec<ServiceUser> =
            execute_batch_concurrent(16, requests_and_clients, |x| async move {
                x.1.request(SERVICE_REQUEST_GETPRELOADEDOWNEDGAMES, x.0)
                    .await
                    .unwrap()
            })
            .await;

        println!("Got responses");

        Ok(())
    }

    fn library_request(locale: &Locale, r#type: ServiceGameProductType, entitlement_enabled: bool, page: u32) -> Result<ServiceGetPreloadedOwnedGamesRequest> {
        Ok(ServiceGetPreloadedOwnedGamesRequestBuilder::default()
            .is_mac(false)
            .locale(locale.to_owned())
            .limit(100)
            .next(((page - 1) * 100).to_string())
            .r#type(r#type)
            .entitlement_enabled(Some(entitlement_enabled))
            .storefronts(vec![
                ServiceStorefront::Ea,
                ServiceStorefront::Steam,
                ServiceStorefront::Epic,
            ])
            .platforms(vec![ServicePlatform::Pc])
            .build()?)
    }
}
