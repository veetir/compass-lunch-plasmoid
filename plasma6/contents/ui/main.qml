import QtQuick 2.15
import QtQuick.Controls 2.15 as QQC2
import QtCore
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.plasmoid 2.0
import org.kde.kirigami 2.20 as Kirigami

import "MenuFormatter.js" as MenuFormatter

PlasmoidItem {
    id: root

    property string apiBaseUrl: "https://www.compass-group.fi/menuapi/feed/json"
    property var baseRestaurantCatalog: [
        { code: "0437", fallbackName: "Snellmania", provider: "compass" },
        { code: "0439", fallbackName: "Tietoteknia", provider: "compass" },
        { code: "0436", fallbackName: "Canthia", provider: "compass" }
    ]
    property var antellRestaurantCatalog: [
        { code: "antell-highway", fallbackName: "Antell Highway", provider: "antell", antellSlug: "highway", antellUrlBase: "https://antell.fi/lounas/kuopio/highway/" },
        { code: "antell-round", fallbackName: "Antell Round", provider: "antell", antellSlug: "round", antellUrlBase: "https://antell.fi/lounas/kuopio/round/" }
    ]
    property var restaurantCatalog: configEnableAntellRestaurants ? baseRestaurantCatalog.concat(antellRestaurantCatalog) : baseRestaurantCatalog

    property var restaurantStates: ({})
    property var requestSerialByCode: ({})
    property var cacheStore: ({})
    property int modelVersion: 0
    property bool initialized: false
    property var supportedIconNames: ["food", "compass", "map-globe", "map-flat"]

    property string activeRestaurantCode: "0437"

    property string configRestaurantCode: {
        var raw = String(Plasmoid.configuration.restaurantCode || Plasmoid.configuration.costNumber || "0437").trim()
        return isKnownRestaurant(raw) ? raw : "0437"
    }
    property string configLanguage: {
        var raw = String(Plasmoid.configuration.language || "fi").toLowerCase()
        return raw === "en" ? "en" : "fi"
    }
    property bool configEnableAntellRestaurants: !!Plasmoid.configuration.enableAntellRestaurants
    property bool configEnableWheelCycle: Plasmoid.configuration.enableWheelCycle !== false
    property int configRefreshMinutes: {
        var raw = Number(Plasmoid.configuration.refreshMinutes)
        if (!isFinite(raw)) {
            return 1440
        }
        raw = Math.floor(raw)
        if (raw < 0) {
            return 1440
        }
        return raw
    }
    property int configManualRefreshToken: Number(Plasmoid.configuration.manualRefreshToken || 0)
    property bool configShowPrices: !!Plasmoid.configuration.showPrices
    property bool configShowStudentPrice: Plasmoid.configuration.showStudentPrice !== false
    property bool configShowStaffPrice: Plasmoid.configuration.showStaffPrice !== false
    property bool configShowGuestPrice: Plasmoid.configuration.showGuestPrice !== false
    property bool configShowAllergens: Plasmoid.configuration.showAllergens !== false
    property bool configHighlightGlutenFree: !!Plasmoid.configuration.highlightGlutenFree
    property bool configHighlightVeg: !!Plasmoid.configuration.highlightVeg
    property bool configHighlightLactoseFree: !!Plasmoid.configuration.highlightLactoseFree
    property string configIconName: {
        var raw = String(Plasmoid.configuration.iconName || "food").trim()
        return supportedIconNames.indexOf(raw) >= 0 ? raw : "food"
    }

    Settings {
        id: cache
        property string cacheBlob: "{}"
    }

    function touchModel() {
        modelVersion += 1
    }

    function restaurantCodes() {
        var list = []
        for (var i = 0; i < restaurantCatalog.length; i++) {
            list.push(String(restaurantCatalog[i].code))
        }
        return list
    }

    function isKnownRestaurant(code) {
        var normalized = String(code || "")
        var codes = restaurantCodes()
        return codes.indexOf(normalized) >= 0
    }

    function restaurantEntryForCode(code) {
        var normalized = String(code || "")
        for (var i = 0; i < restaurantCatalog.length; i++) {
            if (String(restaurantCatalog[i].code) === normalized) {
                return restaurantCatalog[i]
            }
        }
        return null
    }

    function restaurantLabelForCode(code) {
        var normalized = String(code || "")
        for (var i = 0; i < restaurantCatalog.length; i++) {
            if (restaurantCatalog[i].code === normalized) {
                return restaurantCatalog[i].fallbackName
            }
        }
        return "Restaurant " + normalized
    }

    function stateTemplate(code) {
        return {
            restaurantCode: code,
            status: "idle",
            errorMessage: "",
            lastUpdatedEpochMs: 0,
            payloadText: "",
            rawPayload: null,
            todayMenu: null,
            restaurantName: "",
            restaurantUrl: ""
        }
    }

    function ensureStateMaps() {
        var codes = restaurantCodes()
        for (var i = 0; i < codes.length; i++) {
            var code = codes[i]
            if (!restaurantStates[code]) {
                restaurantStates[code] = stateTemplate(code)
            }
            if (!requestSerialByCode[code]) {
                requestSerialByCode[code] = 0
            }
        }
    }

    function resetAllStates() {
        var codes = restaurantCodes()
        var next = {}
        for (var i = 0; i < codes.length; i++) {
            next[codes[i]] = stateTemplate(codes[i])
        }
        restaurantStates = next
        touchModel()
    }

    function stateFor(code) {
        ensureStateMaps()
        var normalized = String(code || "")
        if (!restaurantStates[normalized]) {
            restaurantStates[normalized] = stateTemplate(normalized)
            touchModel()
        }
        return restaurantStates[normalized]
    }

    function formatLastUpdated(epochMs) {
        var value = Number(epochMs) || 0
        if (value <= 0) {
            return ""
        }
        return Qt.formatDateTime(new Date(value), Qt.DefaultLocaleShortDate)
    }

    function syncSettingsLastUpdatedDisplay() {
        var state = stateFor(activeRestaurantCode)
        Plasmoid.configuration.lastUpdatedDisplay = formatLastUpdated(state.lastUpdatedEpochMs)
    }

    function updateState(code, patch) {
        var current = stateFor(code)
        var next = {}
        for (var key in current) {
            next[key] = current[key]
        }
        for (var patchKey in patch) {
            next[patchKey] = patch[patchKey]
        }
        restaurantStates[String(code)] = next
        touchModel()
    }

    function localDateIso(dateObj) {
        var year = dateObj.getFullYear()
        var month = (dateObj.getMonth() + 1).toString()
        var day = dateObj.getDate().toString()

        if (month.length < 2) {
            month = "0" + month
        }
        if (day.length < 2) {
            day = "0" + day
        }

        return year + "-" + month + "-" + day
    }

    function weekdayToken(dateObj) {
        var names = ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"]
        return names[dateObj.getDay()] || "monday"
    }

    function decodeHtmlEntities(text) {
        return String(text || "")
            .replace(/&#x([0-9a-fA-F]+);/g, function(_, hex) {
                return String.fromCharCode(parseInt(hex, 16))
            })
            .replace(/&#([0-9]+);/g, function(_, dec) {
                return String.fromCharCode(parseInt(dec, 10))
            })
            .replace(/&amp;/g, "&")
            .replace(/&lt;/g, "<")
            .replace(/&gt;/g, ">")
            .replace(/&quot;/g, "\"")
            .replace(/&#39;/g, "'")
            .replace(/&nbsp;/g, " ")
    }

    function stripHtmlText(rawHtml) {
        var withoutTags = String(rawHtml || "").replace(/<[^>]*>/g, " ")
        return MenuFormatter.normalizeText(decodeHtmlEntities(withoutTags))
    }

    function parseAntellSections(htmlText) {
        var sections = []
        var sectionRegex = /<section class="menu-section">([\s\S]*?)<\/section>/gi
        var sectionMatch

        while ((sectionMatch = sectionRegex.exec(String(htmlText || ""))) !== null) {
            var sectionHtml = sectionMatch[1]
            var titleMatch = sectionHtml.match(/<h2 class="menu-title">([\s\S]*?)<\/h2>/i)
            var priceMatch = sectionHtml.match(/<h2 class="menu-price">([\s\S]*?)<\/h2>/i)
            var listMatch = sectionHtml.match(/<ul class="menu-list">([\s\S]*?)<\/ul>/i)

            var title = stripHtmlText(titleMatch ? titleMatch[1] : "")
            var price = stripHtmlText(priceMatch ? priceMatch[1] : "")
            var listHtml = listMatch ? listMatch[1] : ""

            var items = []
            var liRegex = /<li[^>]*>([\s\S]*?)<\/li>/gi
            var liMatch
            while ((liMatch = liRegex.exec(listHtml)) !== null) {
                var itemText = stripHtmlText(liMatch[1])
                if (itemText) {
                    items.push(itemText)
                }
            }

            if (items.length === 0) {
                continue
            }

            sections.push({
                sortOrder: sections.length + 1,
                name: title || "Menu",
                price: price,
                components: items
            })
        }

        return sections
    }

    function normalizeAntellTodayMenu(rawPayload) {
        if (!rawPayload || rawPayload.provider !== "antell") {
            return null
        }

        return {
            dateIso: localDateIso(new Date()),
            lunchTime: "",
            menus: parseAntellSections(rawPayload.htmlText)
        }
    }

    function parseAntellPayload(code, htmlText) {
        var entry = restaurantEntryForCode(code)
        var payloadText = String(htmlText || "")
        var locationMatch = payloadText.match(/<div class="menu-location">([\s\S]*?)<\/div>/i)
        var location = stripHtmlText(locationMatch ? locationMatch[1] : "")
        var fallbackName = entry ? String(entry.fallbackName || "Antell") : "Antell"
        var name = location
            ? (location.toLowerCase().indexOf("antell") === 0 ? location : ("Antell " + location))
            : fallbackName
        var url = entry && entry.antellUrlBase ? String(entry.antellUrlBase) : ""
        var rawPayload = {
            provider: "antell",
            htmlText: payloadText,
            restaurantName: name,
            restaurantUrl: url
        }

        return {
            rawPayload: rawPayload,
            todayMenu: normalizeAntellTodayMenu(rawPayload),
            restaurantName: name,
            restaurantUrl: url
        }
    }

    function normalizeMenuEntry(menuEntry) {
        var name = MenuFormatter.normalizeText(menuEntry && menuEntry.Name)
        var price = MenuFormatter.normalizeText(menuEntry && menuEntry.Price)
        var components = []

        var rawComponents = menuEntry && menuEntry.Components
        if (Array.isArray(rawComponents)) {
            for (var i = 0; i < rawComponents.length; i++) {
                var clean = MenuFormatter.normalizeText(rawComponents[i])
                if (clean) {
                    components.push(clean)
                }
            }
        }

        if (!name && components.length === 0) {
            return null
        }

        return {
            sortOrder: Number(menuEntry.SortOrder) || 0,
            name: name || "Menu",
            price: price,
            components: components
        }
    }

    function normalizeTodayMenu(payload) {
        if (!payload || !Array.isArray(payload.MenusForDays)) {
            return null
        }

        var todayIso = localDateIso(new Date())

        for (var i = 0; i < payload.MenusForDays.length; i++) {
            var day = payload.MenusForDays[i]
            if (MenuFormatter.dayKey(day && day.Date) !== todayIso) {
                continue
            }

            var rawSetMenus = Array.isArray(day.SetMenus) ? day.SetMenus.slice(0) : []
            rawSetMenus.sort(function(a, b) {
                return (Number(a.SortOrder) || 0) - (Number(b.SortOrder) || 0)
            })

            var menus = []
            for (var j = 0; j < rawSetMenus.length; j++) {
                var normalized = normalizeMenuEntry(rawSetMenus[j])
                if (normalized) {
                    menus.push(normalized)
                }
            }

            return {
                dateIso: todayIso,
                lunchTime: MenuFormatter.normalizeText(day.LunchTime),
                menus: menus
            }
        }

        return {
            dateIso: todayIso,
            lunchTime: "",
            menus: []
        }
    }

    function cacheKey(code) {
        var entry = restaurantEntryForCode(code)
        if (entry && entry.provider === "antell") {
            return String(code) + "|antell"
        }
        return String(code) + "|" + configLanguage
    }

    function loadCacheStore() {
        try {
            var parsed = JSON.parse(cache.cacheBlob || "{}")
            if (parsed && typeof parsed === "object") {
                cacheStore = parsed
            } else {
                cacheStore = {}
            }
        } catch (e) {
            cacheStore = {}
        }
    }

    function saveCacheEntry(code, payloadText, updatedEpochMs) {
        cacheStore[cacheKey(code)] = {
            payload: payloadText,
            lastUpdatedEpochMs: Number(updatedEpochMs) || 0
        }

        try {
            cache.cacheBlob = JSON.stringify(cacheStore)
        } catch (e) {
        }
    }

    function setErrorStateForCode(code, message) {
        var current = stateFor(code)
        updateState(code, {
            status: current.payloadText ? "stale" : "error",
            errorMessage: message
        })
    }

    function applyPayloadForCode(code, payloadText, fromCache, cachedTimestamp) {
        var entry = restaurantEntryForCode(code)
        var provider = entry && entry.provider ? String(entry.provider) : "compass"
        var parsed = null
        var todayMenu = null
        var restaurantName = ""
        var restaurantUrl = ""

        if (provider === "antell") {
            var antell = parseAntellPayload(code, payloadText)
            parsed = antell.rawPayload
            todayMenu = antell.todayMenu
            restaurantName = antell.restaurantName
            restaurantUrl = antell.restaurantUrl
        } else {
            try {
                parsed = JSON.parse(payloadText)
            } catch (e) {
                setErrorStateForCode(code, "Invalid JSON payload")
                return false
            }

            if (!parsed || !Array.isArray(parsed.MenusForDays)) {
                setErrorStateForCode(code, "Missing MenusForDays in payload")
                return false
            }

            if (parsed.ErrorText) {
                setErrorStateForCode(code, MenuFormatter.normalizeText(parsed.ErrorText))
                return false
            }

            todayMenu = normalizeTodayMenu(parsed)
            restaurantName = MenuFormatter.normalizeText(parsed.RestaurantName) || "Compass Lunch"
            restaurantUrl = MenuFormatter.normalizeText(parsed.RestaurantUrl)
        }

        var updatedMs = fromCache ? (Number(cachedTimestamp) || 0) : Date.now()

        updateState(code, {
            status: fromCache ? "stale" : "ok",
            errorMessage: "",
            lastUpdatedEpochMs: updatedMs,
            payloadText: payloadText,
            rawPayload: parsed,
            todayMenu: todayMenu,
            restaurantName: restaurantName,
            restaurantUrl: restaurantUrl
        })

        if (String(code) === activeRestaurantCode) {
            syncSettingsLastUpdatedDisplay()
        }

        if (!fromCache) {
            saveCacheEntry(code, payloadText, updatedMs)
        }

        return true
    }

    function loadCachedPayloadsForCurrentLanguage() {
        var codes = restaurantCodes()
        for (var i = 0; i < codes.length; i++) {
            var code = codes[i]
            var entry = cacheStore[cacheKey(code)]
            if (!entry || !entry.payload) {
                continue
            }
            applyPayloadForCode(code, entry.payload, true, entry.lastUpdatedEpochMs)
        }
    }

    function refreshTodayMenusFromPayload() {
        var codes = restaurantCodes()
        for (var i = 0; i < codes.length; i++) {
            var code = codes[i]
            var state = stateFor(code)
            if (!state.rawPayload) {
                continue
            }
            var refreshedMenu = state.rawPayload.provider === "antell"
                ? normalizeAntellTodayMenu(state.rawPayload)
                : normalizeTodayMenu(state.rawPayload)
            updateState(code, {
                todayMenu: refreshedMenu
            })
        }
    }

    function buildRequestUrl(code) {
        var entry = restaurantEntryForCode(code)
        if (!entry) {
            return ""
        }

        if (entry.provider === "antell") {
            return String(entry.antellUrlBase)
                + "?print_lunch_day="
                + encodeURIComponent(weekdayToken(new Date()))
                + "&print_lunch_list_day=1"
        }

        return apiBaseUrl + "?costNumber=" + encodeURIComponent(String(code)) + "&language=" + encodeURIComponent(configLanguage)
    }

    function fetchRestaurant(code, manual) {
        if (!isKnownRestaurant(code)) {
            return
        }

        var normalized = String(code)
        requestSerialByCode[normalized] = (requestSerialByCode[normalized] || 0) + 1
        var requestSerial = requestSerialByCode[normalized]

        var current = stateFor(normalized)
        if (!current.payloadText) {
            updateState(normalized, {
                status: "loading",
                errorMessage: ""
            })
        }

        var requestUrl = buildRequestUrl(normalized)
        if (!requestUrl) {
            setErrorStateForCode(normalized, "Unsupported restaurant provider")
            return
        }

        var xhr = new XMLHttpRequest()
        xhr.open("GET", requestUrl)
        xhr.timeout = manual ? 15000 : 10000

        xhr.onreadystatechange = function() {
            if (xhr.readyState !== XMLHttpRequest.DONE) {
                return
            }
            if (requestSerial !== requestSerialByCode[normalized]) {
                return
            }

            if (xhr.status >= 200 && xhr.status < 300) {
                applyPayloadForCode(normalized, xhr.responseText, false, 0)
            } else {
                setErrorStateForCode(normalized, "HTTP " + xhr.status)
            }
        }

        xhr.onerror = function() {
            if (requestSerial !== requestSerialByCode[normalized]) {
                return
            }
            setErrorStateForCode(normalized, "Network error")
        }

        xhr.ontimeout = function() {
            if (requestSerial !== requestSerialByCode[normalized]) {
                return
            }
            setErrorStateForCode(normalized, "Request timed out")
        }

        xhr.send()
    }

    function refreshAllRestaurants(manual) {
        var codes = restaurantCodes()
        for (var i = 0; i < codes.length; i++) {
            fetchRestaurant(codes[i], manual)
        }
    }

    function scheduleMidnightTimer() {
        var now = new Date()
        var next = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, 0, 1, 0, 0)
        var msUntil = next.getTime() - now.getTime()
        midnightTimer.interval = Math.max(60000, msUntil)
        midnightTimer.restart()
    }

    function openConfigureAction() {
        var configureAction = Plasmoid.action("configure")
        if (configureAction && configureAction.enabled) {
            configureAction.trigger()
        }
    }

    function cycleRestaurant(step) {
        if (!configEnableWheelCycle) {
            return
        }

        var codes = restaurantCodes()
        if (codes.length < 2) {
            return
        }

        var idx = codes.indexOf(activeRestaurantCode)
        if (idx < 0) {
            idx = 0
        }

        var nextIdx = (idx + step + codes.length) % codes.length
        activeRestaurantCode = codes[nextIdx]

        if (!stateFor(activeRestaurantCode).payloadText) {
            fetchRestaurant(activeRestaurantCode, false)
        }
    }

    function tooltipMainText() {
        var state = stateFor(activeRestaurantCode)
        return state.restaurantName || "Compass Lunch"
    }

    function tooltipSubText() {
        var state = stateFor(activeRestaurantCode)
        var entry = restaurantEntryForCode(activeRestaurantCode)
        var isCompassProvider = !!entry && entry.provider === "compass"
        return MenuFormatter.buildTooltipSubText(
            configLanguage,
            state.status,
            state.errorMessage,
            state.lastUpdatedEpochMs,
            state.todayMenu,
            configShowPrices,
            configShowStudentPrice,
            configShowStaffPrice,
            configShowGuestPrice,
            isCompassProvider,
            configShowAllergens,
            configHighlightGlutenFree,
            configHighlightVeg,
            configHighlightLactoseFree
        )
    }

    function tooltipSubTextRich() {
        var state = stateFor(activeRestaurantCode)
        var entry = restaurantEntryForCode(activeRestaurantCode)
        var isCompassProvider = !!entry && entry.provider === "compass"
        return MenuFormatter.buildTooltipSubTextRich(
            configLanguage,
            state.status,
            state.errorMessage,
            state.lastUpdatedEpochMs,
            state.todayMenu,
            configShowPrices,
            configShowStudentPrice,
            configShowStaffPrice,
            configShowGuestPrice,
            isCompassProvider,
            configShowAllergens,
            configHighlightGlutenFree,
            configHighlightVeg,
            configHighlightLactoseFree
        )
    }

    function activeIconName() {
        var state = stateFor(activeRestaurantCode)
        return (state.status === "error" || state.status === "stale") ? "dialog-warning" : configIconName
    }

    function bootstrapData() {
        ensureStateMaps()
        activeRestaurantCode = configRestaurantCode
        loadCacheStore()
        loadCachedPayloadsForCurrentLanguage()
        refreshAllRestaurants(false)
        syncSettingsLastUpdatedDisplay()
    }

    onConfigRestaurantCodeChanged: {
        activeRestaurantCode = configRestaurantCode
        if (!stateFor(activeRestaurantCode).payloadText) {
            fetchRestaurant(activeRestaurantCode, false)
        }
        syncSettingsLastUpdatedDisplay()
    }

    onActiveRestaurantCodeChanged: syncSettingsLastUpdatedDisplay()

    onConfigLanguageChanged: {
        resetAllStates()
        activeRestaurantCode = configRestaurantCode
        loadCacheStore()
        loadCachedPayloadsForCurrentLanguage()
        refreshAllRestaurants(false)
        syncSettingsLastUpdatedDisplay()
    }

    onConfigEnableAntellRestaurantsChanged: {
        resetAllStates()
        activeRestaurantCode = configRestaurantCode
        loadCacheStore()
        loadCachedPayloadsForCurrentLanguage()
        refreshAllRestaurants(false)
        syncSettingsLastUpdatedDisplay()
    }

    onConfigRefreshMinutesChanged: {
        refreshTimer.interval = Math.max(1, configRefreshMinutes) * 60 * 1000
        if (configRefreshMinutes > 0) {
            refreshTimer.restart()
        } else {
            refreshTimer.stop()
        }
    }
    onConfigManualRefreshTokenChanged: {
        if (!initialized) {
            return
        }
        refreshAllRestaurants(true)
    }

    Component.onCompleted: {
        bootstrapData()
        scheduleMidnightTimer()
        initialized = true
    }

    Timer {
        id: refreshTimer
        interval: Math.max(1, root.configRefreshMinutes) * 60 * 1000
        running: root.configRefreshMinutes > 0
        repeat: true
        onTriggered: root.refreshAllRestaurants(false)
    }

    Timer {
        id: midnightTimer
        repeat: false
        running: false
        onTriggered: {
            root.refreshTodayMenusFromPayload()
            root.refreshAllRestaurants(false)
            root.scheduleMidnightTimer()
        }
    }

    Plasmoid.icon: {
        var _ = modelVersion
        return activeIconName()
    }
    Plasmoid.status: PlasmaCore.Types.ActiveStatus
    toolTipTextFormat: Text.RichText
    toolTipMainText: {
        var _ = modelVersion
        return tooltipMainText()
    }
    toolTipSubText: {
        var _ = modelVersion
        return tooltipSubTextRich()
    }

    Plasmoid.onActivated: {
        Plasmoid.expanded = true
    }

    compactRepresentation: Item {
        id: compactRoot
        implicitWidth: PlasmaCore.Units.iconSizes.smallMedium
        implicitHeight: PlasmaCore.Units.iconSizes.smallMedium

        Kirigami.Icon {
            anchors.fill: parent
            source: Plasmoid.icon
            active: compactMouse.containsMouse
        }

        MouseArea {
            id: compactMouse
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.MiddleButton

            onClicked: {
                if (mouse.button === Qt.MiddleButton) {
                    var state = root.stateFor(root.activeRestaurantCode)
                    if (state.restaurantUrl) {
                        Qt.openUrlExternally(state.restaurantUrl)
                        return
                    }
                }
                Plasmoid.expanded = true
            }

            onWheel: {
                if (!root.configEnableWheelCycle) {
                    return
                }
                if (wheel.angleDelta.y > 0) {
                    root.cycleRestaurant(-1)
                } else if (wheel.angleDelta.y < 0) {
                    root.cycleRestaurant(1)
                }
                wheel.accepted = true
            }
        }
    }

    fullRepresentation: Item {
        implicitWidth: 480
        implicitHeight: 380

        Rectangle {
            anchors.fill: parent
            color: PlasmaCore.Theme.backgroundColor
            radius: Kirigami.Units.smallSpacing * 2
            border.width: 1
            border.color: PlasmaCore.Theme.highlightColor

            Flickable {
                id: flick
                anchors.fill: parent
                anchors.margins: Kirigami.Units.smallSpacing * 2
                contentWidth: width
                contentHeight: fullText.paintedHeight
                clip: true

                QQC2.Label {
                    id: fullText
                    width: flick.width
                    wrapMode: Text.Wrap
                    textFormat: Text.RichText
                    text: root.tooltipSubTextRich()
                }
            }
        }
    }
}
