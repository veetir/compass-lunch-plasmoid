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
            menuDateIso: "",
            providerDateValid: false,
            isTodayFresh: false,
            consecutiveFailures: 0,
            nextRetryEpochMs: 0,
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

    function todayIso() {
        return localDateIso(new Date())
    }

    function isStateFreshForToday(state) {
        if (!state) {
            return false
        }
        return !!state.providerDateValid && MenuFormatter.normalizeText(state.menuDateIso) === todayIso()
    }

    function retryDelayMinutes(failureCount) {
        var count = Math.max(1, Number(failureCount) || 1)
        if (count <= 1) {
            return 5
        }
        if (count === 2) {
            return 10
        }
        return 15
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

    function parseAntellMenuDateIso(menuDateText) {
        var clean = MenuFormatter.normalizeText(menuDateText)
        if (!clean) {
            return ""
        }

        var parts = clean.match(/(\d{1,2})\.(\d{1,2})(?:\.(\d{2,4}))?/)
        if (!parts) {
            return ""
        }

        var day = Number(parts[1])
        var month = Number(parts[2])
        if (!isFinite(day) || !isFinite(month) || day < 1 || day > 31 || month < 1 || month > 12) {
            return ""
        }

        function buildCandidate(yearNumber) {
            var candidate = new Date(yearNumber, month - 1, day)
            if (candidate.getFullYear() !== yearNumber || candidate.getMonth() !== month - 1 || candidate.getDate() !== day) {
                return null
            }
            return candidate
        }

        if (parts[3]) {
            var explicitYear = Number(parts[3])
            if (!isFinite(explicitYear)) {
                return ""
            }
            if (explicitYear < 100) {
                explicitYear += 2000
            }
            var datedCandidate = buildCandidate(explicitYear)
            return datedCandidate ? localDateIso(datedCandidate) : ""
        }

        var now = new Date()
        var nowMidnight = new Date(now.getFullYear(), now.getMonth(), now.getDate())
        var years = [now.getFullYear() - 1, now.getFullYear(), now.getFullYear() + 1]
        var best = null
        var bestDistance = Number.MAX_VALUE

        for (var i = 0; i < years.length; i++) {
            var candidate = buildCandidate(years[i])
            if (!candidate) {
                continue
            }
            var distance = Math.abs(candidate.getTime() - nowMidnight.getTime())
            if (distance < bestDistance) {
                bestDistance = distance
                best = candidate
            }
        }

        return best ? localDateIso(best) : ""
    }

    function normalizeAntellTodayMenu(rawPayload) {
        if (!rawPayload || rawPayload.provider !== "antell" || !rawPayload.providerDateValid) {
            return null
        }

        var menuDate = MenuFormatter.normalizeText(rawPayload.menuDateIso)
        if (!menuDate) {
            return null
        }

        return {
            dateIso: menuDate,
            lunchTime: "",
            menus: parseAntellSections(rawPayload.htmlText)
        }
    }

    function parseAntellPayload(code, htmlText) {
        var entry = restaurantEntryForCode(code)
        var payloadText = String(htmlText || "")
        var locationMatch = payloadText.match(/<div class="menu-location">([\s\S]*?)<\/div>/i)
        var menuDateMatch = payloadText.match(/<div class="menu-date">([\s\S]*?)<\/div>/i)
        var location = stripHtmlText(locationMatch ? locationMatch[1] : "")
        var menuDateText = stripHtmlText(menuDateMatch ? menuDateMatch[1] : "")
        var menuDateIso = parseAntellMenuDateIso(menuDateText)
        var isDateToday = menuDateIso && menuDateIso === todayIso()
        var fallbackName = entry ? String(entry.fallbackName || "Antell") : "Antell"
        var name = location
            ? (location.toLowerCase().indexOf("antell") === 0 ? location : ("Antell " + location))
            : fallbackName
        var url = entry && entry.antellUrlBase ? String(entry.antellUrlBase) : ""
        var rawPayload = {
            provider: "antell",
            htmlText: payloadText,
            menuDateText: menuDateText,
            menuDateIso: menuDateIso,
            providerDateValid: !!isDateToday,
            restaurantName: name,
            restaurantUrl: url
        }

        return {
            rawPayload: rawPayload,
            todayMenu: normalizeAntellTodayMenu(rawPayload),
            menuDateIso: menuDateIso,
            providerDateValid: !!isDateToday,
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

        var currentDateIso = todayIso()

        for (var i = 0; i < payload.MenusForDays.length; i++) {
            var day = payload.MenusForDays[i]
            if (MenuFormatter.dayKey(day && day.Date) !== currentDateIso) {
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
                todayMenu: {
                    dateIso: currentDateIso,
                    lunchTime: MenuFormatter.normalizeText(day.LunchTime),
                    menus: menus
                },
                menuDateIso: currentDateIso,
                providerDateValid: true
            }
        }

        return {
            todayMenu: null,
            menuDateIso: "",
            providerDateValid: false
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

    function dateMismatchMessage() {
        return "Date mismatch: expected " + todayIso()
    }

    function setErrorStateForCode(code, message) {
        var current = stateFor(code)
        if (isStateFreshForToday(current)) {
            updateState(code, {
                status: "ok",
                errorMessage: "",
                consecutiveFailures: 0,
                nextRetryEpochMs: 0
            })
            return
        }

        var failureCount = (Number(current.consecutiveFailures) || 0) + 1
        updateState(code, {
            status: current.payloadText ? "stale" : "error",
            errorMessage: message,
            isTodayFresh: false,
            consecutiveFailures: failureCount,
            nextRetryEpochMs: Date.now() + retryDelayMinutes(failureCount) * 60 * 1000
        })
        retryTimer.start()
    }

    function applyPayloadForCode(code, payloadText, fromCache, cachedTimestamp) {
        var entry = restaurantEntryForCode(code)
        var provider = entry && entry.provider ? String(entry.provider) : "compass"
        var parsed = null
        var todayMenu = null
        var menuDateIso = ""
        var providerDateValid = false
        var restaurantName = ""
        var restaurantUrl = ""

        if (provider === "antell") {
            var antell = parseAntellPayload(code, payloadText)
            parsed = antell.rawPayload
            todayMenu = antell.todayMenu
            menuDateIso = antell.menuDateIso
            providerDateValid = antell.providerDateValid
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

            var normalizedCompass = normalizeTodayMenu(parsed)
            if (!normalizedCompass) {
                setErrorStateForCode(code, "Invalid menu payload")
                return false
            }

            todayMenu = normalizedCompass.todayMenu
            menuDateIso = normalizedCompass.menuDateIso
            providerDateValid = normalizedCompass.providerDateValid
            restaurantName = MenuFormatter.normalizeText(parsed.RestaurantName) || "Compass Lunch"
            restaurantUrl = MenuFormatter.normalizeText(parsed.RestaurantUrl)
        }

        var updatedMs = fromCache ? (Number(cachedTimestamp) || 0) : Date.now()
        var freshToday = !!providerDateValid && menuDateIso === todayIso()
        var current = stateFor(code)
        var failureCount = Number(current.consecutiveFailures) || 0

        if (!freshToday && !fromCache) {
            failureCount += 1
        } else if (freshToday) {
            failureCount = 0
        }

        var nextRetryEpochMs = Number(current.nextRetryEpochMs) || 0
        if (freshToday) {
            nextRetryEpochMs = 0
        } else if (!fromCache) {
            nextRetryEpochMs = Date.now() + retryDelayMinutes(failureCount) * 60 * 1000
        } else if (!isFinite(nextRetryEpochMs) || nextRetryEpochMs < 0) {
            nextRetryEpochMs = 0
        }

        updateState(code, {
            status: freshToday ? "ok" : "stale",
            errorMessage: freshToday ? "" : dateMismatchMessage(),
            lastUpdatedEpochMs: updatedMs,
            payloadText: payloadText,
            rawPayload: parsed,
            todayMenu: todayMenu,
            menuDateIso: menuDateIso,
            providerDateValid: !!providerDateValid,
            isTodayFresh: freshToday,
            consecutiveFailures: failureCount,
            nextRetryEpochMs: nextRetryEpochMs,
            restaurantName: restaurantName,
            restaurantUrl: restaurantUrl
        })

        if (!freshToday && !fromCache) {
            retryTimer.start()
        }

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

    function rederiveStateFromCachedPayload() {
        var codes = restaurantCodes()
        for (var i = 0; i < codes.length; i++) {
            var code = codes[i]
            var state = stateFor(code)
            if (!state.payloadText) {
                continue
            }
            applyPayloadForCode(code, state.payloadText, true, state.lastUpdatedEpochMs)
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
        var current = stateFor(normalized)
        if (!manual && current.status === "loading") {
            return
        }

        requestSerialByCode[normalized] = (requestSerialByCode[normalized] || 0) + 1
        var requestSerial = requestSerialByCode[normalized]

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

    function evaluateFreshnessAndRefresh(forceNetwork, manual) {
        var codes = restaurantCodes()
        for (var i = 0; i < codes.length; i++) {
            var code = codes[i]
            if (forceNetwork || manual) {
                fetchRestaurant(code, !!manual)
                continue
            }

            var state = stateFor(code)
            if (!isStateFreshForToday(state)) {
                fetchRestaurant(code, false)
            }
        }
    }

    function processDueRetries() {
        var nowMs = Date.now()
        var codes = restaurantCodes()
        var hasPendingRetry = false

        for (var i = 0; i < codes.length; i++) {
            var code = codes[i]
            var state = stateFor(code)
            var dueMs = Number(state.nextRetryEpochMs) || 0

            if (!dueMs || isStateFreshForToday(state)) {
                continue
            }

            hasPendingRetry = true
            if (dueMs <= nowMs) {
                fetchRestaurant(code, false)
            }
        }

        if (!hasPendingRetry) {
            retryTimer.stop()
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

        if (!isStateFreshForToday(stateFor(activeRestaurantCode))) {
            fetchRestaurant(activeRestaurantCode, false)
        }
    }

    function tooltipMainText() {
        var state = stateFor(activeRestaurantCode)
        var title = state.restaurantName || "Compass Lunch"
        if (state.status === "stale" && !state.isTodayFresh) {
            return "[STALE] " + title
        }
        return title
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
        evaluateFreshnessAndRefresh(false, false)
        syncSettingsLastUpdatedDisplay()
    }

    onConfigRestaurantCodeChanged: {
        activeRestaurantCode = configRestaurantCode
        if (!isStateFreshForToday(stateFor(activeRestaurantCode))) {
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
        evaluateFreshnessAndRefresh(false, false)
        syncSettingsLastUpdatedDisplay()
    }

    onConfigEnableAntellRestaurantsChanged: {
        resetAllStates()
        activeRestaurantCode = configRestaurantCode
        loadCacheStore()
        loadCachedPayloadsForCurrentLanguage()
        evaluateFreshnessAndRefresh(false, false)
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
        evaluateFreshnessAndRefresh(true, true)
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
        onTriggered: root.evaluateFreshnessAndRefresh(false, false)
    }

    Timer {
        id: retryTimer
        interval: 30000
        running: false
        repeat: true
        onTriggered: root.processDueRetries()
    }

    Timer {
        id: midnightTimer
        repeat: false
        running: false
        onTriggered: {
            root.rederiveStateFromCachedPayload()
            root.evaluateFreshnessAndRefresh(false, false)
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
