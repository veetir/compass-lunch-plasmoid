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

function main() {
  checkFixture("output-en.json", "Lunch");
  checkFixture("output-fi.json", "Annosruoka");
  process.stdout.write("Parser checks passed for output-en.json and output-fi.json\n");
}

main();
