#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Compass,
    CompassRss,
    Antell,
    HuomenJson,
}

#[derive(Debug, Clone, Copy)]
pub struct Restaurant {
    pub code: &'static str,
    pub name: &'static str,
    pub provider: Provider,
    pub antell_slug: Option<&'static str>,
    pub rss_cost_number: Option<&'static str>,
    pub huomen_api_base: Option<&'static str>,
    pub url: Option<&'static str>,
}

const CORE_RESTAURANTS: [Restaurant; 5] = [
    Restaurant {
        code: "0437",
        name: "Snellmania",
        provider: Provider::Compass,
        antell_slug: None,
        rss_cost_number: None,
        huomen_api_base: None,
        url: None,
    },
    Restaurant {
        code: "snellari-rss",
        name: "Cafe Snellari",
        provider: Provider::CompassRss,
        antell_slug: None,
        rss_cost_number: Some("4370"),
        huomen_api_base: None,
        url: Some(
            "https://www.compass-group.fi/ravintolat-ja-ruokalistat/foodco/kaupungit/kuopio/cafe-snellari/",
        ),
    },
    Restaurant {
        code: "0436",
        name: "Canthia",
        provider: Provider::Compass,
        antell_slug: None,
        rss_cost_number: None,
        huomen_api_base: None,
        url: None,
    },
    Restaurant {
        code: "0439",
        name: "Tietoteknia",
        provider: Provider::Compass,
        antell_slug: None,
        rss_cost_number: None,
        huomen_api_base: None,
        url: None,
    },
    Restaurant {
        code: "huomen-bioteknia",
        name: "Hyvä Huomen Bioteknia",
        provider: Provider::HuomenJson,
        antell_slug: None,
        rss_cost_number: None,
        huomen_api_base: Some(
            "https://europe-west1-luncher-7cf76.cloudfunctions.net/api/v1/week/a96b7ccf-2c3d-432a-8504-971dbb6d55d3/active",
        ),
        url: Some("https://hyvahuomen.fi/bioteknia/"),
    },
];

const ANTELL_RESTAURANTS: [Restaurant; 2] = [
    Restaurant {
        code: "antell-round",
        name: "Antell Round",
        provider: Provider::Antell,
        antell_slug: Some("round"),
        rss_cost_number: None,
        huomen_api_base: None,
        url: Some("https://antell.fi/lounas/kuopio/round/"),
    },
    Restaurant {
        code: "antell-highway",
        name: "Antell Highway",
        provider: Provider::Antell,
        antell_slug: Some("highway"),
        rss_cost_number: None,
        huomen_api_base: None,
        url: Some("https://antell.fi/lounas/kuopio/highway/"),
    },
];

pub fn available_restaurants(enable_antell: bool) -> Vec<Restaurant> {
    let mut list = Vec::new();
    list.extend_from_slice(&CORE_RESTAURANTS);
    if enable_antell {
        list.extend_from_slice(&ANTELL_RESTAURANTS);
    }
    list
}

pub fn restaurant_for_code(code: &str, enable_antell: bool) -> Restaurant {
    let list = available_restaurants(enable_antell);
    list.into_iter()
        .find(|r| r.code == code)
        .unwrap_or(CORE_RESTAURANTS[0])
}

pub fn provider_key(provider: Provider) -> &'static str {
    match provider {
        Provider::Compass => "compass",
        Provider::CompassRss => "compass-rss",
        Provider::Antell => "antell",
        Provider::HuomenJson => "huomen-json",
    }
}
