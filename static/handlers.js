function enableFullscreen () {
    if(document.fullscreenEnabled) {
    // Get the element you want to make full screen
    var elem = document.documentElement; // This will make the whole page full screen

    // Request full screen
    if(elem.requestFullscreen) {
        elem.requestFullscreen();
    }
    }
}

function toggleVis () {
    let doToggle = (el) => {
        if (el) {
            let v = el.style.visibility;
            if (v === "visible" || v === "") {
                el.style.visibility = "hidden";
            } else {
                el.style.visibility = "visible";
            }
        }
    };
    doToggle(document.getElementById("section-definitions"));
    doToggle(document.getElementById("section-links"));
    doToggle(document.getElementById("section-examples"));
    doToggle(document.getElementById("variants-content"));
    document.querySelectorAll("#lookup-header rt").forEach(e => doToggle(e));
}

function onBodyKeypress () {
    if (event.ctrlKey) {
        return;
    }
    let bulk_okay = ".line:hover .bulk-okay";
    let favourite = ".line:hover .favourite-btn";
    let targets = {
        "u": bulk_okay,
        "i": favourite,
        "j": "#sidebar-fail-button",
        "k": "#sidebar-hard-button",
        "l": "#sidebar-okay-button",
        ";": "#sidebar-easy-button",
        " ": ".variant:hover",
    }
    targets["f"] = targets["j"];
    targets["d"] = targets["k"];
    targets["s"] = targets["l"];
    targets["a"] = targets[";"];
    targets["r"] = targets["u"];
    targets["e"] = targets["i"];
    if (event.key in targets) {
        event.preventDefault();
        console.log(`running handler for key ${event.key}`);
        const target = targets[event.key];
        if (htmx.find(target)) {
            // TODO use custom event
            htmx.trigger(target, "click");
        }
    } else {
        console.log(`no handler for key ${event.key}`);
    }
}
