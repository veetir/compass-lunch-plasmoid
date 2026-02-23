#!/usr/bin/env node

const fs = require("fs");
const path = require("path");

function normalizeText(value) {
  if (value === null || value === undefined) {
    return "";
  }
  return String(value).replace(/\s*\n+\s*/g, " ").replace(/\s+/g, " ").trim();
}

function dayKey(dateString) {
  const clean = normalizeText(dateString);
  if (!clean) {
    return "";
  }
  return clean.split("T")[0] || "";
}

function localDateIso(dateObj) {
  const year = dateObj.getFullYear();
  const month = String(dateObj.getMonth() + 1).padStart(2, "0");
  const day = String(dateObj.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function normalizeMenusForDay(day) {
  const rawMenus = Array.isArray(day.SetMenus) ? [...day.SetMenus] : [];
  rawMenus.sort((a, b) => (Number(a.SortOrder) || 0) - (Number(b.SortOrder) || 0));

  return rawMenus
    .map((entry) => {
      const name = normalizeText(entry.Name) || "Menu";
      const price = normalizeText(entry.Price);
      const components = Array.isArray(entry.Components)
        ? entry.Components.map((item) => normalizeText(item)).filter(Boolean)
        : [];

      if (!name && components.length === 0) {
        return null;
      }

      return {
        sortOrder: Number(entry.SortOrder) || 0,
        name,
        price,
        components,
      };
    })
    .filter(Boolean);
}

function normalizeCompassToday(payload, targetDate) {
  if (!payload || !Array.isArray(payload.MenusForDays)) {
    return null;
  }

  const match = payload.MenusForDays.find((day) => dayKey(day.Date) === targetDate);
  if (!match) {
    return {
      todayMenu: null,
      menuDateIso: "",
      providerDateValid: false,
    };
  }

  return {
    todayMenu: {
      dateIso: targetDate,
      lunchTime: normalizeText(match.LunchTime),
      menus: normalizeMenusForDay(match),
    },
    menuDateIso: targetDate,
    providerDateValid: true,
  };
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

function readFixture(name) {
  const fixturePath = path.join(__dirname, "fixtures", name);
  const raw = fs.readFileSync(fixturePath, "utf8");
  return JSON.parse(raw);
}

function readTextFixture(name) {
  const fixturePath = path.join(__dirname, "fixtures", name);
  return fs.readFileSync(fixturePath, "utf8");
}

function decodeHtmlEntities(value) {
  return String(value)
    .replace(/&#x([0-9a-fA-F]+);/g, (_, hex) => String.fromCharCode(parseInt(hex, 16)))
    .replace(/&#([0-9]+);/g, (_, dec) => String.fromCharCode(parseInt(dec, 10)))
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, "\"")
    .replace(/&#39;/g, "'")
    .replace(/&nbsp;/g, " ");
}

function stripHtmlText(value) {
  return normalizeText(decodeHtmlEntities(String(value).replace(/<[^>]*>/g, " ")));
}

function parseAntellSections(htmlText) {
  const sections = [];
  const sectionRegex = /<section class="menu-section">([\s\S]*?)<\/section>/gi;
  let sectionMatch;

  while ((sectionMatch = sectionRegex.exec(String(htmlText))) !== null) {
    const sectionHtml = sectionMatch[1];
    const titleMatch = sectionHtml.match(/<h2 class="menu-title">([\s\S]*?)<\/h2>/i);
    const priceMatch = sectionHtml.match(/<h2 class="menu-price">([\s\S]*?)<\/h2>/i);
    const listMatch = sectionHtml.match(/<ul class="menu-list">([\s\S]*?)<\/ul>/i);

    const title = stripHtmlText(titleMatch ? titleMatch[1] : "");
    const price = stripHtmlText(priceMatch ? priceMatch[1] : "");
    const listHtml = listMatch ? listMatch[1] : "";

    const items = [];
    const itemRegex = /<li[^>]*>([\s\S]*?)<\/li>/gi;
    let itemMatch;
    while ((itemMatch = itemRegex.exec(listHtml)) !== null) {
      const item = stripHtmlText(itemMatch[1]);
      if (item) {
        items.push(item);
      }
    }

    if (items.length === 0) {
      continue;
    }

    sections.push({
      title: title || "Menu",
      price,
      items,
    });
  }

  return sections;
}

function parseAntellMenuDateIso(menuDateText, nowDate) {
  const clean = normalizeText(menuDateText);
  if (!clean) {
    return "";
  }

  const parts = clean.match(/(\d{1,2})\.(\d{1,2})(?:\.(\d{2,4}))?/);
  if (!parts) {
    return "";
  }

  const day = Number(parts[1]);
  const month = Number(parts[2]);
  if (!Number.isFinite(day) || !Number.isFinite(month) || day < 1 || day > 31 || month < 1 || month > 12) {
    return "";
  }

  function buildCandidate(yearNumber) {
    const candidate = new Date(yearNumber, month - 1, day);
    if (
      candidate.getFullYear() !== yearNumber ||
      candidate.getMonth() !== month - 1 ||
      candidate.getDate() !== day
    ) {
      return null;
    }
    return candidate;
  }

  if (parts[3]) {
    let explicitYear = Number(parts[3]);
    if (!Number.isFinite(explicitYear)) {
      return "";
    }
    if (explicitYear < 100) {
      explicitYear += 2000;
    }
    const explicit = buildCandidate(explicitYear);
    return explicit ? localDateIso(explicit) : "";
  }

  const now = nowDate instanceof Date ? nowDate : new Date();
  const nowMidnight = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const years = [now.getFullYear() - 1, now.getFullYear(), now.getFullYear() + 1];
  let best = null;
  let bestDistance = Number.MAX_VALUE;

  for (const year of years) {
    const candidate = buildCandidate(year);
    if (!candidate) {
      continue;
    }
    const distance = Math.abs(candidate.getTime() - nowMidnight.getTime());
    if (distance < bestDistance) {
      bestDistance = distance;
      best = candidate;
    }
  }

  return best ? localDateIso(best) : "";
}

function parseAntellMeta(htmlText, nowDate) {
  const raw = String(htmlText || "");
  const dateMatch = raw.match(/<div class="menu-date">([\s\S]*?)<\/div>/i);
  const menuDateText = stripHtmlText(dateMatch ? dateMatch[1] : "");
  const menuDateIso = parseAntellMenuDateIso(menuDateText, nowDate);
  const expectedIso = localDateIso(nowDate instanceof Date ? nowDate : new Date());
  return {
    menuDateText,
    menuDateIso,
    providerDateValid: !!menuDateIso && menuDateIso === expectedIso,
  };
}

function parseRssTagRaw(xmlText, tagName) {
  const regex = new RegExp(`<${tagName}(?:\\s+[^>]*)?>([\\s\\S]*?)<\\/${tagName}>`, "i");
  const match = String(xmlText || "").match(regex);
  return match ? String(match[1] || "") : "";
}

function parseRssDateIso(dateText) {
  const clean = normalizeText(dateText);
  if (!clean) {
    return "";
  }

  const parts = clean.match(/(\d{1,2})[-./](\d{1,2})[-./](\d{2,4})/);
  if (!parts) {
    return "";
  }

  const day = Number(parts[1]);
  const month = Number(parts[2]);
  let year = Number(parts[3]);
  if (!Number.isFinite(day) || !Number.isFinite(month) || !Number.isFinite(year)) {
    return "";
  }
  if (year < 100) {
    year += 2000;
  }
  if (day < 1 || day > 31 || month < 1 || month > 12) {
    return "";
  }

  const candidate = new Date(year, month - 1, day);
  if (candidate.getFullYear() !== year || candidate.getMonth() !== month - 1 || candidate.getDate() !== day) {
    return "";
  }
  return localDateIso(candidate);
}

function parseRssComponents(descriptionRaw) {
  const decoded = decodeHtmlEntities(String(descriptionRaw || ""));
  const components = [];
  const paragraphRegex = /<p[^>]*>([\s\S]*?)<\/p>/gi;
  let paragraphMatch;

  while ((paragraphMatch = paragraphRegex.exec(decoded)) !== null) {
    const line = stripHtmlText(paragraphMatch[1]);
    if (line) {
      components.push(line);
    }
  }

  if (components.length === 0) {
    const fallback = stripHtmlText(decoded);
    if (fallback) {
      components.push(fallback);
    }
  }

  return components;
}

function parseRssMeta(rssText, nowDate) {
  const raw = String(rssText || "");
  const channelRaw = parseRssTagRaw(raw, "channel");
  const itemMatch = String(channelRaw || raw).match(/<item\b[^>]*>([\s\S]*?)<\/item>/i);
  const itemRaw = itemMatch ? String(itemMatch[1] || "") : "";

  const itemTitle = stripHtmlText(parseRssTagRaw(itemRaw, "title"));
  const itemGuid = stripHtmlText(parseRssTagRaw(itemRaw, "guid"));
  const itemLink = stripHtmlText(parseRssTagRaw(itemRaw, "link"));
  const descriptionRaw = parseRssTagRaw(itemRaw, "description");
  const menuDateIso = parseRssDateIso(itemTitle) || parseRssDateIso(itemGuid);
  const expectedIso = localDateIso(nowDate instanceof Date ? nowDate : new Date());

  return {
    itemTitle,
    itemGuid,
    itemLink,
    menuDateIso,
    providerDateValid: !!menuDateIso && menuDateIso === expectedIso,
    components: parseRssComponents(descriptionRaw),
  };
}

function retryDelayMinutes(failureCount) {
  const count = Math.max(1, Number(failureCount) || 1);
  if (count <= 1) {
    return 5;
  }
  if (count === 2) {
    return 10;
  }
  return 15;
}

function checkCompassFixture(name, expectedMenuName) {
  const payload = readFixture(name);

  assert(normalizeText(payload.RestaurantName).length > 0, `${name}: missing RestaurantName`);
  assert(Array.isArray(payload.MenusForDays), `${name}: MenusForDays is not an array`);
  assert(payload.MenusForDays.length > 0, `${name}: MenusForDays is empty`);

  const fresh = normalizeCompassToday(payload, "2026-02-19");
  assert(fresh && fresh.providerDateValid, `${name}: expected providerDateValid on 2026-02-19`);
  assert(fresh.menuDateIso === "2026-02-19", `${name}: unexpected menuDateIso: ${fresh.menuDateIso}`);
  assert(fresh.todayMenu, `${name}: expected todayMenu on 2026-02-19`);
  assert(fresh.todayMenu.lunchTime === "10:30–14:30", `${name}: unexpected lunch time: ${fresh.todayMenu.lunchTime}`);
  assert(fresh.todayMenu.menus.length > 0, `${name}: no menus on 2026-02-19`);
  assert(fresh.todayMenu.menus[0].name === expectedMenuName, `${name}: first menu mismatch: ${fresh.todayMenu.menus[0].name}`);

  for (const menu of fresh.todayMenu.menus) {
    for (const component of menu.components) {
      assert(!component.includes("\n"), `${name}: newline remained in component: ${component}`);
    }
  }

  const closedDay = normalizeCompassToday(payload, "2026-02-22");
  assert(closedDay && closedDay.providerDateValid, `${name}: 2026-02-22 should still be a valid day`);
  assert(closedDay.todayMenu, `${name}: expected closed-day todayMenu object`);
  assert(closedDay.todayMenu.menus.length === 0, `${name}: expected no menus on 2026-02-22`);
  assert(closedDay.todayMenu.lunchTime === "", `${name}: expected empty lunchTime on 2026-02-22`);

  const staleDay = normalizeCompassToday(payload, "2026-02-23");
  assert(staleDay && !staleDay.providerDateValid, `${name}: expected stale when day is missing`);
  assert(staleDay.todayMenu === null, `${name}: expected null todayMenu for missing day`);
  assert(staleDay.menuDateIso === "", `${name}: expected empty menuDateIso when day missing`);
}

function checkAntellFixture(name, expectedFirstTitle, expectedFirstItem, expectedSections) {
  const html = readTextFixture(name);
  const sections = parseAntellSections(html);

  assert(sections.length === expectedSections, `${name}: expected ${expectedSections} parsed sections, got ${sections.length}`);
  assert(sections[0].title === expectedFirstTitle, `${name}: unexpected first title: ${sections[0].title}`);
  assert(sections[0].items[0] === expectedFirstItem, `${name}: unexpected first item: ${sections[0].items[0]}`);

  for (const section of sections) {
    for (const item of section.items) {
      assert(item.length > 0, `${name}: empty parsed item`);
    }
  }

  const matchingDate = new Date(2026, 1, 20);
  const validMeta = parseAntellMeta(html, matchingDate);
  assert(validMeta.menuDateText.length > 0, `${name}: missing parsed menu-date text`);
  assert(validMeta.menuDateIso === "2026-02-20", `${name}: expected parsed menu date 2026-02-20`);
  assert(validMeta.providerDateValid, `${name}: expected providerDateValid on matching local date`);

  const mismatchMeta = parseAntellMeta(html, new Date(2026, 1, 21));
  assert(!mismatchMeta.providerDateValid, `${name}: expected mismatch on non-matching date`);

  const missingDateHtml = html.replace(/<div class="menu-date">[\s\S]*?<\/div>/i, "");
  const missingMeta = parseAntellMeta(missingDateHtml, matchingDate);
  assert(missingMeta.menuDateIso === "", `${name}: missing menu-date should produce empty ISO`);
  assert(!missingMeta.providerDateValid, `${name}: missing menu-date should be invalid`);
}

function checkRssFixture(name) {
  const rss = readTextFixture(name);
  const todayMeta = parseRssMeta(rss, new Date(2026, 1, 23));
  assert(todayMeta.providerDateValid, `${name}: expected valid date on 2026-02-23`);
  assert(todayMeta.menuDateIso === "2026-02-23", `${name}: unexpected date: ${todayMeta.menuDateIso}`);
  assert(todayMeta.itemLink.includes("cafe-snellari"), `${name}: missing restaurant link`);
  assert(todayMeta.components.length >= 4, `${name}: expected at least 4 menu lines`);
  assert(
    todayMeta.components[0] === "Juustoista peruna-pinaattisosekeittoa *, A, G, ILM, L",
    `${name}: unexpected first line: ${todayMeta.components[0]}`
  );
  assert(
    todayMeta.components.some((line) => line.includes("katkarapuja")),
    `${name}: expected katkarapuja line in components`
  );

  const staleMeta = parseRssMeta(rss, new Date(2026, 1, 24));
  assert(!staleMeta.providerDateValid, `${name}: expected stale date on 2026-02-24`);

  const noDateRss = rss
    .replace(/#23-02-2026/i, "#no-date")
    .replace(/Maanantai,\s*23-02-2026/i, "Maanantai");
  const missingDateMeta = parseRssMeta(noDateRss, new Date(2026, 1, 23));
  assert(missingDateMeta.menuDateIso === "", `${name}: expected empty date when RSS has no date`);
  assert(!missingDateMeta.providerDateValid, `${name}: expected invalid providerDateValid when date is missing`);
}

function checkRetryDelays() {
  assert(retryDelayMinutes(1) === 5, "retry delay for first failure should be 5");
  assert(retryDelayMinutes(2) === 10, "retry delay for second failure should be 10");
  assert(retryDelayMinutes(3) === 15, "retry delay for third failure should be 15");
  assert(retryDelayMinutes(8) === 15, "retry delay should stay at 15 after third failure");
}

function main() {
  checkCompassFixture("output-en.json", "Lunch");
  checkCompassFixture("output-fi.json", "Annosruoka");
  checkAntellFixture(
    "antell-highway-friday-snippet.html",
    "Pääruoaksi",
    "Hoisin-kastikkeella maustettuja nyhtöpossuhodareita (A, L, M)",
    3
  );
  checkAntellFixture(
    "antell-round-friday-snippet.html",
    "Kotiruokalounas",
    "Perinteiset lihapyörykät mummonkastikkeella(G oma)",
    3
  );
  checkRssFixture("snellari.rss");
  checkRetryDelays();
  process.stdout.write("Parser checks passed for Compass, Antell and RSS freshness rules\n");
}

main();
