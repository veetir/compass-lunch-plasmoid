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

function getDay(payload, targetDate) {
  if (!payload || !Array.isArray(payload.MenusForDays)) {
    return null;
  }

  const match = payload.MenusForDays.find((day) => dayKey(day.Date) === targetDate);
  if (!match) {
    return null;
  }

  return {
    dateIso: targetDate,
    lunchTime: normalizeText(match.LunchTime),
    menus: normalizeMenusForDay(match),
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

function checkFixture(name, expectedMenuName) {
  const payload = readFixture(name);

  assert(normalizeText(payload.RestaurantName).length > 0, `${name}: missing RestaurantName`);
  assert(Array.isArray(payload.MenusForDays), `${name}: MenusForDays is not an array`);
  assert(payload.MenusForDays.length > 0, `${name}: MenusForDays is empty`);

  const day = getDay(payload, "2026-02-19");
  assert(day, `${name}: 2026-02-19 day missing`);
  assert(day.lunchTime === "10:30–14:30", `${name}: unexpected lunch time: ${day.lunchTime}`);
  assert(day.menus.length > 0, `${name}: no menus on 2026-02-19`);
  assert(day.menus[0].name === expectedMenuName, `${name}: first menu mismatch: ${day.menus[0].name}`);

  for (const menu of day.menus) {
    for (const component of menu.components) {
      assert(!component.includes("\n"), `${name}: newline remained in component: ${component}`);
    }
  }

  const closedDay = getDay(payload, "2026-02-22");
  assert(closedDay, `${name}: 2026-02-22 day missing`);
  assert(closedDay.menus.length === 0, `${name}: expected no menus on 2026-02-22`);
  assert(closedDay.lunchTime === "", `${name}: expected empty lunchTime on 2026-02-22`);
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
}

function main() {
  checkFixture("output-en.json", "Lunch");
  checkFixture("output-fi.json", "Annosruoka");
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
  process.stdout.write("Parser checks passed for Compass and Antell fixtures\n");
}

main();
