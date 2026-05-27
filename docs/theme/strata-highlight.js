(function () {
    "use strict";

    function registerStrata(hljs) {
        if (!hljs || !hljs.registerLanguage || hljs.getLanguage("strata")) {
            return;
        }

        hljs.registerLanguage("strata", function (hljs) {
            var ident = "[A-Za-z_][A-Za-z0-9_]*";
            var lowerIdent = "\\b[a-z_][A-Za-z0-9_]*\\b";
            var typePattern = "\\b[A-Z][A-Za-z0-9_]*\\b";
            var scalarLiteral = "\\b-?\\d+_(?:u8|u16|u32|u64|i8|i16|i32|i64)\\b";
            var scalarTypes = "Bool Unit U8 U16 U32 U64 I8 I16 I32 I64";
            var genericTypes =
                "Cap List Map Option ProcResult ProcessRef Result SendError Spawn SpawnError";
            var resultVariants =
                "BackendUnavailable Continue Crashed Denied Err Exhausted Full MailboxClosed None Ok Panic Some Stop Stopped True False";
            var typeMode = {
                className: "type",
                begin: typePattern,
                relevance: 0,
            };
            var genericTypeMode = {
                className: "built_in",
                begin:
                    "\\b(?:" +
                    genericTypes.replace(/ /g, "|") +
                    "|" +
                    scalarTypes.replace(/ /g, "|") +
                    ")\\b(?=\\s*(?:<|\\[|\\{|\\(|,|;|\\]|\\)|\\}|>))",
                relevance: 0,
            };
            var literalVariantMode = {
                className: "literal",
                begin:
                    "\\b(?:" +
                    resultVariants.replace(/ /g, "|") +
                    ")\\b(?=\\s*(?:\\(|[,;\\]\\)}]|$))",
                relevance: 0,
            };
            var numberModes = [
                {
                    className: "number",
                    begin: scalarLiteral,
                },
                {
                    className: "number",
                    begin: "\\b\\d+\\b",
                    relevance: 0,
                },
            ];
            var titleMode = {
                className: "title",
                begin: ident,
                relevance: 0,
            };
            var authorityNameMode = {
                className: "title",
                begin: "\\b" + ident + "\\b(?=\\s*:)",
                relevance: 0,
            };
            var processTitleMode = {
                className: "title",
                begin: typePattern,
                relevance: 0,
            };
            var fieldMode = {
                className: "attr",
                begin: lowerIdent + "(?=\\s*:)",
                relevance: 0,
            };
            var annotationMode = {
                className: "meta",
                begin: "@(?:det|nondet|[A-Za-z_][A-Za-z0-9_]*)",
            };
            var effectMode = {
                className: "meta",
                begin: "!\\s*\\[",
                end: "\\]",
                contains: [
                    {
                        className: "keyword",
                        begin: "\\b(?:emit|spawn|send)\\b",
                    },
                ],
            };
            var mayBehaviorMode = {
                className: "meta",
                begin: "~\\s*\\[",
                end: "\\]",
            };
            var typeContains = [genericTypeMode, typeMode].concat(numberModes);

            return {
                name: "Strata",
                aliases: ["str"],
                keywords: {
                    keyword:
                        "as authority bounded else emit enum fn for if in let mailbox match module mut proc record return security send spawn type var",
                    built_in:
                        genericTypes + " " + scalarTypes,
                    literal: resultVariants,
                },
                contains: [
                    hljs.COMMENT("//", "$"),
                    hljs.COMMENT("/\\*", "\\*/"),
                    {
                        className: "string",
                        begin: "\"",
                        end: "\"",
                        illegal: "\\n",
                    },
                    annotationMode,
                    effectMode,
                    mayBehaviorMode,
                    {
                        className: "meta",
                        beginKeywords: "module",
                        end: ";",
                        contains: [
                            {
                                className: "title",
                                begin: lowerIdent,
                                relevance: 0,
                            },
                        ],
                    },
                    {
                        className: "class",
                        beginKeywords: "record enum",
                        end: "(?=[{;])",
                        excludeEnd: true,
                        contains: [processTitleMode],
                    },
                    {
                        className: "class",
                        beginKeywords: "proc",
                        end: "(?=\\{)",
                        excludeEnd: true,
                        contains: [
                            processTitleMode,
                            {
                                className: "keyword",
                                begin: "\\b(?:mailbox|bounded)\\b",
                            },
                        ].concat(numberModes),
                    },
                    {
                        className: "meta",
                        beginKeywords: "authority",
                        end: ";",
                        contains: [authorityNameMode].concat(typeContains),
                    },
                    {
                        className: "meta",
                        beginKeywords: "type",
                        end: ";",
                        contains: [
                            {
                                className: "attr",
                                begin: "\\b(?:State|Msg)\\b",
                            },
                        ].concat(typeContains),
                    },
                    {
                        className: "function",
                        beginKeywords: "fn",
                        end: "(?=\\()",
                        excludeEnd: true,
                        contains: [titleMode],
                    },
                    {
                        className: "operator",
                        begin: "->|=>|==|!=|&&|\\|\\||\\.\\.|[=<>!]",
                        relevance: 0,
                    },
                    fieldMode,
                    literalVariantMode,
                    genericTypeMode,
                    typeMode,
                ].concat(numberModes),
                lexemes: ident,
                case_insensitive: false,
                disableAutodetect: true,
                illegal: "</",
            };
        });
    }

    function hasHighlightMarkup(block) {
        return Boolean(block.querySelector("span[class^='hljs-']"));
    }

    function highlightBlock(block) {
        if (typeof window.hljs.highlightElement === "function") {
            window.hljs.highlightElement(block);
            return;
        }

        if (typeof window.hljs.highlightBlock === "function") {
            window.hljs.highlightBlock(block);
        }
    }

    function highlightStrataBlocks() {
        registerStrata(window.hljs);

        if (!window.hljs || !window.hljs.getLanguage || !window.hljs.getLanguage("strata")) {
            return;
        }

        document.querySelectorAll("pre code.language-strata").forEach(function (block) {
            if (!hasHighlightMarkup(block)) {
                highlightBlock(block);
            }
            block.classList.add("hljs");
        });
    }

    registerStrata(window.hljs);
    window.addEventListener("load", highlightStrataBlocks);
})();
