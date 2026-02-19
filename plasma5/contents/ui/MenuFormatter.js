.pragma library

function normalizeText(value) {
    if (value === null || value === undefined) {
        return "";
    }
    return String(value).replace(/\s*\n+\s*/g, " ").replace(/\s+/g, " ").trim();
}

function escapeHtml(value) {
    var text = normalizeText(value);
    return text
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/\"/g, "&quot;")
        .replace(/'/g, "&#39;");
}

function dayKey(dateString) {
    var clean = normalizeText(dateString);
    if (!clean) {
        return "";
    }
    var parts = clean.split("T");
    return parts[0] || "";
}

function formatDisplayDate(dateIso, language) {
    var iso = normalizeText(dateIso);
    var match = iso.match(/^(\d{4})-(\d{2})-(\d{2})$/);
    if (!match) {
        return iso;
    }

    var year = match[1];
    var month = parseInt(match[2], 10);
    var day = parseInt(match[3], 10);

    if (language === "fi") {
        return day + "." + month + "." + year;
    }

    return month + "/" + day + "/" + year;
}

function dateAndTimeLine(todayMenu, language) {
    if (!todayMenu) {
        return "";
    }

    var datePart = formatDisplayDate(todayMenu.dateIso, language);
    var timePart = normalizeText(todayMenu.lunchTime);

    if (datePart && timePart) {
        return datePart + " " + timePart;
    }
    if (datePart) {
        return datePart;
    }
    return timePart;
}

function textFor(language, key) {
    var fi = {
        loading: "Ladataan ruokalistaa...",
        noMenu: "Talle paivalle ei ole lounaslistaa.",
        stale: "Ei verkkoyhteytta. Naytetaan viimeisin tallennettu lista",
        fetchError: "Paivitysvirhe"
    };

    var en = {
        loading: "Loading menu...",
        noMenu: "No lunch menu available for today.",
        stale: "Offline. Showing last cached menu",
        fetchError: "Fetch error"
    };

    var dict = language === "fi" ? fi : en;
    return dict[key] || key;
}

function menuHeading(menu, showPrices) {
    var heading = normalizeText(menu.name);
    if (!heading) {
        heading = "Menu";
    }

    var price = normalizeText(menu.price);
    if (showPrices && price) {
        return heading + " - " + price;
    }

    return heading;
}

function splitComponentSuffix(component) {
    var text = normalizeText(component);
    var match = text.match(/^(.*\S)\s+(\([^()]*\))$/);
    if (!match) {
        return {
            main: text,
            suffix: ""
        };
    }
    return {
        main: normalizeText(match[1]),
        suffix: normalizeText(match[2])
    };
}

function buildTooltipSubText(language, fetchState, errorMessage, lastUpdatedEpochMs, todayMenu, showPrices) {
    var lines = [];

    if (!todayMenu && fetchState === "loading") {
        lines.push(textFor(language, "loading"));
    }

    var dateLine = dateAndTimeLine(todayMenu, language);
    if (dateLine) {
        lines.push(dateLine);
    }

    if (todayMenu && todayMenu.menus && todayMenu.menus.length > 0) {
        for (var i = 0; i < todayMenu.menus.length; i++) {
            var menu = todayMenu.menus[i];
            lines.push(menuHeading(menu, showPrices));
            var components = menu.components || [];
            for (var j = 0; j < components.length; j++) {
                var component = normalizeText(components[j]);
                if (component) {
                    lines.push("  - " + component);
                }
            }
        }
    } else if (fetchState !== "loading") {
        lines.push(textFor(language, "noMenu"));
    }

    if (fetchState === "stale") {
        lines.push("");
        lines.push(textFor(language, "stale"));
    }

    var cleanError = normalizeText(errorMessage);
    if (cleanError && fetchState !== "ok") {
        lines.push(textFor(language, "fetchError") + ": " + cleanError);
    }

    return lines.join("\n");
}

function buildTooltipSubTextRich(language, fetchState, errorMessage, lastUpdatedEpochMs, todayMenu, showPrices) {
    var lines = [];

    if (!todayMenu && fetchState === "loading") {
        lines.push(escapeHtml(textFor(language, "loading")));
    }

    var dateLine = dateAndTimeLine(todayMenu, language);
    if (dateLine) {
        lines.push("<b>" + escapeHtml(dateLine) + "</b>");
    }

    if (todayMenu && todayMenu.menus && todayMenu.menus.length > 0) {
        for (var i = 0; i < todayMenu.menus.length; i++) {
            var menu = todayMenu.menus[i];
            lines.push("<b>" + escapeHtml(menuHeading(menu, showPrices)) + "</b>");

            var components = menu.components || [];
            for (var j = 0; j < components.length; j++) {
                var component = normalizeText(components[j]);
                if (component) {
                    var parts = splitComponentSuffix(component);
                    var componentLine = "&nbsp;&nbsp;&nbsp;▸ " + escapeHtml(parts.main);
                    if (parts.suffix) {
                        componentLine += " <small>" + escapeHtml(parts.suffix) + "</small>";
                    }
                    lines.push(componentLine);
                }
            }
        }
    } else if (fetchState !== "loading") {
        lines.push(escapeHtml(textFor(language, "noMenu")));
    }

    if (fetchState === "stale") {
        lines.push("&nbsp;");
        lines.push(escapeHtml(textFor(language, "stale")));
    }

    var cleanError = normalizeText(errorMessage);
    if (cleanError && fetchState !== "ok") {
        lines.push(escapeHtml(textFor(language, "fetchError") + ": " + cleanError));
    }

    return lines.join("<br/>");
}
