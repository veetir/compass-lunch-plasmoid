#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Compass,
    Antell,
}

#[derive(Debug, Clone, Copy)]
pub struct Restaurant {
    pub code: &'static str,
    pub name: &'static str,
    pub provider: Provider,
    pub antell_slug: Option<&'static str>,
    pub url: Option<&'static str>,
}

const COMPASS_RESTAURANTS: [Restaurant; 3] = [
    Restaurant {
        code: "0437",
        name: "Snellmania",
        provider: Provider::Compass,
        antell_slug: None,
        url: None,
    },
    Restaurant {
        code: "0439",
        name: "Tietoteknia",
        provider: Provider::Compass,
        antell_slug: None,
        url: None,
    },
    Restaurant {
        code: "0436",
        name: "Canthia",
        provider: Provider::Compass,
        antell_slug: None,
        url: None,
    },
];

const ANTELL_RESTAURANTS: [Restaurant; 2] = [
    Restaurant {
        code: "antell-highway",
        name: "Antell Highway",
        provider: Provider::Antell,
        antell_slug: Some("highway"),
        url: Some("https://antell.fi/lounas/kuopio/highway/"),
    },
    Restaurant {
        code: "antell-round",
        name: "Antell Round",
        provider: Provider::Antell,
        antell_slug: Some("round"),
        url: Some("https://antell.fi/lounas/kuopio/round/"),
    },
];

pub fn available_restaurants(enable_antell: bool) -> Vec<Restaurant> {
    let mut list = Vec::new();
    list.extend_from_slice(&COMPASS_RESTAURANTS);
    if enable_antell {
        list.extend_from_slice(&ANTELL_RESTAURANTS);
    }
    list
}

pub fn restaurant_for_code(code: &str, enable_antell: bool) -> Restaurant {
    let list = available_restaurants(enable_antell);
    list.into_iter()
        .find(|r| r.code == code)
        .unwrap_or(COMPASS_RESTAURANTS[0])
}

pub fn is_antell_code(code: &str) -> bool {
    ANTELL_RESTAURANTS.iter().any(|r| r.code == code)
}

pub fn provider_key(provider: Provider) -> &'static str {
    match provider {
        Provider::Compass => "compass",
        Provider::Antell => "antell",
    }
}
