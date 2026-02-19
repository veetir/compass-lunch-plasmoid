use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ApiResponse {
    #[serde(rename = "RestaurantName")]
    pub restaurant_name: Option<String>,
    #[serde(rename = "RestaurantUrl")]
    pub restaurant_url: Option<String>,
    #[serde(rename = "MenusForDays")]
    pub menus_for_days: Option<Vec<ApiMenuDay>>,
    #[serde(rename = "ErrorText")]
    pub error_text: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ApiMenuDay {
    #[serde(rename = "Date")]
    pub date: Option<String>,
    #[serde(rename = "LunchTime")]
    pub lunch_time: Option<String>,
    #[serde(rename = "SetMenus")]
    pub set_menus: Option<Vec<ApiSetMenu>>,
}

#[derive(Debug, Deserialize)]
pub struct ApiSetMenu {
    #[serde(rename = "SortOrder")]
    pub sort_order: Option<i32>,
    #[serde(rename = "Name")]
    pub name: Option<String>,
    #[serde(rename = "Price")]
    pub price: Option<String>,
    #[serde(rename = "Components")]
    pub components: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct TodayMenu {
    pub date_iso: String,
    pub lunch_time: String,
    pub menus: Vec<MenuGroup>,
}

#[derive(Debug, Clone)]
pub struct MenuGroup {
    pub name: String,
    pub price: String,
    pub components: Vec<String>,
}
