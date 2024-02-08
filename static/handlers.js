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
