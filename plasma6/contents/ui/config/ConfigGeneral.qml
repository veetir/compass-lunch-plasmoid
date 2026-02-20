import QtQuick 2.15
import QtQuick.Controls 2.15 as QQC2
import QtQuick.Layouts 1.15

Item {
    id: page

    property string cfg_restaurantCode: "0437"
    property alias cfg_refreshMinutes: refreshSpin.value
    property int cfg_manualRefreshToken: 0
    property alias cfg_showPrices: showPricesCheck.checked
    property string cfg_iconName: "food"
    property alias cfg_showAllergens: showAllergensCheck.checked
    property alias cfg_highlightGlutenFree: highlightGlutenFreeCheck.checked
    property alias cfg_highlightVeg: highlightVegCheck.checked
    property alias cfg_highlightLactoseFree: highlightLactoseFreeCheck.checked
    property alias cfg_enableWheelCycle: wheelCycleCheck.checked
    property string cfg_lastUpdatedDisplay: ""
    property string cfg_language: "fi"

    function restaurantIndexForCode(code) {
        var list = restaurantCombo.model
        for (var i = 0; i < list.length; i++) {
            if (list[i].code === code) {
                return i
            }
        }
        return 0
    }

    function syncRestaurantCombo() {
        var idx = restaurantIndexForCode(cfg_restaurantCode)
        if (restaurantCombo.currentIndex !== idx) {
            restaurantCombo.currentIndex = idx
        }
        cfg_restaurantCode = restaurantCombo.model[restaurantCombo.currentIndex].code
    }

    function syncLanguageCombo() {
        var idx = languageCombo.model.indexOf(cfg_language)
        if (idx < 0) {
            idx = 0
            cfg_language = languageCombo.model[0]
        }
        if (languageCombo.currentIndex !== idx) {
            languageCombo.currentIndex = idx
        }
    }

    function iconIndexForName(name) {
        var list = iconCombo.model
        for (var i = 0; i < list.length; i++) {
            if (list[i].name === name) {
                return i
            }
        }
        return 0
    }

    function syncIconCombo() {
        var idx = iconIndexForName(cfg_iconName)
        if (iconCombo.currentIndex !== idx) {
            iconCombo.currentIndex = idx
        }
        if (iconCombo.currentIndex >= 0) {
            cfg_iconName = iconCombo.model[iconCombo.currentIndex].name
        }
    }

    onCfg_restaurantCodeChanged: syncRestaurantCombo()
    onCfg_languageChanged: syncLanguageCombo()
    onCfg_iconNameChanged: syncIconCombo()

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 12

        QQC2.Label {
            text: "Favorite restaurant"
        }

        QQC2.ComboBox {
            id: restaurantCombo
            Layout.fillWidth: true
            textRole: "label"
            model: [
                { code: "0437", label: "Ita-Suomen yliopisto/Snellmania (0437)" },
                { code: "0439", label: "Tietoteknia (0439)" },
                { code: "0436", label: "Ita-Suomen yliopisto/Canthia (0436)" }
            ]
            onCurrentIndexChanged: {
                if (currentIndex >= 0) {
                    cfg_restaurantCode = model[currentIndex].code
                }
            }
            Component.onCompleted: page.syncRestaurantCombo()
        }

        QQC2.Label {
            text: "Language"
        }

        QQC2.ComboBox {
            id: languageCombo
            Layout.fillWidth: true
            model: ["fi", "en"]
            onCurrentTextChanged: cfg_language = currentText
            Component.onCompleted: page.syncLanguageCombo()
        }

        QQC2.Label {
            text: "Automatic refresh interval (minutes)"
        }

        QQC2.SpinBox {
            id: refreshSpin
            from: 0
            to: 10080
            stepSize: 60
        }

        QQC2.CheckBox {
            id: showPricesCheck
            text: "Show prices"
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            QQC2.Label {
                text: "Tray icon"
            }

            QQC2.ComboBox {
                id: iconCombo
                Layout.fillWidth: true
                textRole: "label"
                model: [
                    { name: "food", label: "Food (default)" },
                    { name: "compass", label: "Compass" },
                    { name: "map-globe", label: "Globe" },
                    { name: "map-flat", label: "Map" }
                ]
                onCurrentIndexChanged: {
                    if (currentIndex >= 0) {
                        cfg_iconName = model[currentIndex].name
                    }
                }
                Component.onCompleted: page.syncIconCombo()
            }
        }

        QQC2.CheckBox {
            id: showAllergensCheck
            text: "Show allergens"
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 10
            enabled: showAllergensCheck.checked
            opacity: enabled ? 1.0 : 0.55

            QQC2.Label {
                text: "Highlight"
            }

            QQC2.CheckBox {
                id: highlightGlutenFreeCheck
                text: "G"
            }

            QQC2.CheckBox {
                id: highlightVegCheck
                text: "Veg"
            }

            QQC2.CheckBox {
                id: highlightLactoseFreeCheck
                text: "L"
            }
        }

        QQC2.CheckBox {
            id: wheelCycleCheck
            text: "Use mouse wheel on tray icon to switch restaurant"
        }

        QQC2.Button {
            text: "Refresh menus now"
            onClicked: cfg_manualRefreshToken = cfg_manualRefreshToken + 1
        }

        QQC2.Label {
            text: "Last successful update"
        }

        QQC2.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: cfg_lastUpdatedDisplay.length > 0 ? cfg_lastUpdatedDisplay : "No successful update yet"
        }

        Item {
            Layout.fillHeight: true
        }
    }
}
