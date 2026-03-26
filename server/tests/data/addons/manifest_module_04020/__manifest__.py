{
    "name": "Manifest Module 04018",
    "version": "1.0",
    "depends": [],
    "assets": {
        "web.bundle": [
            ("include", "path"),
            ("include", "path", "path"), #OLS04020
            ("append", "path"),
            ("append", "path", "path"), #OLS04020
            ("remove", "path"),
            ("remove", "path", "path"), #OLS04020
            ("prepend", "path"),
            ("prepend", "path", "path"), #OLS04020
            ("before", "path"), #OLS04020
            ("before", "path", "path"),
            ("before", "path", "path", "path"), #OLS04020
            ("after", "path"), #OLS04020
            ("after", "path", "path"),
            ("after", "path", "path", "path"), #OLS04020
            ("replace", "path"), #OLS04020
            ("replace", "path", "path"),
            ("replace", "path", "path", "path"), #OLS04020
        ],
    },
    "installable": True,
}