import { defineConfig } from 'vitepress';
import { withMermaid } from 'vitepress-plugin-mermaid';
import valkyrieGrammar from './valkyrie.tmLanguage.json' with { type: 'json' };

const config = defineConfig({
    title: 'Valkyrie Language',
    description: 'Valkyrie - A modern programming language',
    ignoreDeadLinks: true,

    locales: {
        root: {
            label: '繁体中文',
            lang: 'zh-Hant',
            link: '/zh-hant/',
            themeConfig: {
                nav: [
                    { text: '首页', link: '/zh-hant/' },
                    {
                        text: '快速开始',
                        items: [
                            { text: '快速开始', link: '/zh-hant/guide/' },
                            { text: '查看示例', link: '/zh-hant/examples/' },
                        ],
                    },
                    {
                        text: '语言规范',
                        items: [
                            { text: '核心概念', link: '/zh-hant/language/' },
                            { text: '维护指南', link: '/zh-hant/maintenance/' },
                        ],
                    },
                    { text: '常见问题', link: '/zh-hant/faq' },
                ],
                sidebar: {
                    '/zh-hant/guide/': [
                        {
                            text: '快速开始',
                            items: [
                                { text: '概述', link: '/zh-hant/guide/' },
                                { text: '功能特性', link: '/zh-hant/guide/features' },
                            ],
                        },
                    ],
                    '/zh-hant/language/': [
                        {
                            text: '语言规范',
                            items: [
                                { text: '概述', link: '/zh-hant/language/' },
                                { text: '字面量', link: '/zh-hant/language/literals' },
                                { text: '控制流', link: '/zh-hant/language/control-flow' },
                                { text: '函数定义', link: '/zh-hant/language/definitions' },
                                {
                                    text: '类型系统',
                                    link: '/zh-hant/language/type-system/',
                                    items: [
                                        { text: '基本类型', link: '/zh-hant/language/type-system/' },
                                        { text: '高阶类型', link: '/zh-hant/language/type-system/hkt' },
                                        { text: '类型函数', link: '/zh-hant/language/type-system/type-function' },
                                    ],
                                },
                                {
                                    text: '对象式编程',
                                    link: '/zh-hant/language/object-oriented/',
                                    items: [
                                        { text: '对象模型', link: '/zh-hant/language/object-oriented/' },
                                        { text: '匿名类', link: '/zh-hant/language/object-oriented/anonymous-classes' },
                                        { text: '值类', link: '/zh-hant/language/object-oriented/value-class' },
                                        { text: '属性系统', link: '/zh-hant/language/object-oriented/property' },
                                        { text: '继承', link: '/zh-hant/language/object-oriented/inheritance' },
                                        { text: 'Trait 系统', link: '/zh-hant/language/object-oriented/trait-system' },
                                        { text: '事件系统', link: '/zh-hant/language/object-oriented/events' },
                                        { text: '神经网络', link: '/zh-hant/language/object-oriented/neural' },
                                        { text: '组件系统', link: '/zh-hant/language/object-oriented/widget' },
                                    ],
                                },
                                {
                                    text: '函数式编程',
                                    link: '/zh-hant/language/function-oriented/',
                                    items: [
                                        { text: '概述', link: '/zh-hant/language/function-oriented/' },
                                        {
                                            text: '匿名函数',
                                            link: '/zh-hant/language/function-oriented/anonymous-functions',
                                        },
                                        { text: '模式匹配', link: '/zh-hant/language/function-oriented/pattern-match' },
                                    ],
                                },
                                {
                                    text: '效应式编程',
                                    link: '/zh-hant/language/effect-system/',
                                    items: [
                                        { text: 'Effect 系统', link: '/zh-hant/language/effect-system/' },
                                        { text: '错误处理', link: '/zh-hant/language/effect-system/error-handler' },
                                        { text: '生成器', link: '/zh-hant/language/effect-system/generator' },
                                        { text: '协程', link: '/zh-hant/language/effect-system/coroutine' },
                                        { text: '面向切面编程', link: '/zh-hant/language/effect-system/aop' },
                                        { text: '控制反转', link: '/zh-hant/language/effect-system/ioc' },
                                    ],
                                },
                                {
                                    text: '元编程',
                                    link: '/zh-hant/language/meta-programming/',
                                    items: [
                                        { text: '概述', link: '/zh-hant/language/meta-programming/' },
                                        { text: '宏系统', link: '/zh-hant/language/meta-programming/macro' },
                                        { text: '多继承', link: '/zh-hant/language/meta-programming/inheritance' },
                                    ],
                                },
                                { text: '模块系统', link: '/zh-hant/language/modules' },
                                { text: 'Trait 系统', link: '/zh-hant/language/trait-system' },
                            ],
                        },
                    ],
                    '/zh-hant/maintenance/': [
                        {
                            text: '维护指南',
                            items: [
                                { text: '概述', link: '/zh-hant/maintenance/' },
                                { text: 'Project Architecture', link: '/zh-hant/maintenance/project-architecture' },
                                {
                                    text: 'Salsa Incremental Compilation',
                                    link: '/zh-hant/maintenance/salsa-incremental',
                                },
                                { text: 'Miette Error Handling', link: '/zh-hant/maintenance/miette-error-handling' },
                                { text: 'Execution Models', link: '/zh-hant/maintenance/execution-models' },
                            ],
                        },
                    ],
                    '/zh-hant/examples/': [
                        {
                            text: '示例集合',
                            items: [
                                { text: '概述', link: '/zh-hant/examples/' },
                                {
                                    text: '网页开发',
                                    link: '/zh-hant/examples/web-development/',
                                    items: [
                                        { text: '概述', link: '/zh-hant/examples/web-development/' },
                                        {
                                            text: 'XML 语法 (X-Grammar)',
                                            link: '/zh-hant/examples/web-development/x-grammar',
                                        },
                                        {
                                            text: '原生语法 (V-Grammar)',
                                            link: '/zh-hant/examples/web-development/v-grammar',
                                        },
                                        { text: '界面组件', link: '/zh-hant/examples/web-development/widget' },
                                        { text: '事件处理', link: '/zh-hant/examples/web-development/events' },
                                    ],
                                },
                                {
                                    text: '游戏开发',
                                    link: '/zh-hant/examples/game-development/',
                                    items: [
                                        { text: '概述', link: '/zh-hant/examples/game-development/' },
                                        { text: 'ECS 系统', link: '/zh-hant/examples/game-development/ecs' },
                                        {
                                            text: '着色器编程',
                                            link: '/zh-hant/examples/game-development/graphics-shader',
                                        },
                                        { text: 'GPU 计算', link: '/zh-hant/examples/game-development/gpu-compute' },
                                    ],
                                },
                                {
                                    text: '嵌入式开发',
                                    link: '/zh-hant/examples/embedded-development/',
                                    items: [
                                        { text: '概述', link: '/zh-hant/examples/embedded-development/' },
                                        {
                                            text: '数字电路',
                                            link: '/zh-hant/examples/embedded-development/digital-circuits',
                                        },
                                    ],
                                },
                            ],
                        },
                    ],
                },
            },
        },
        en: {
            label: 'English',
            lang: 'en-US',
            link: '/en-us/',
            themeConfig: {
                nav: [
                    { text: 'Home', link: '/en-us/' },
                    {
                        text: 'Getting Started',
                        items: [
                            { text: 'Quick Start', link: '/en-us/guide/' },
                            { text: 'Examples', link: '/en-us/examples/' },
                        ],
                    },
                    {
                        text: 'Reference',
                        items: [
                            { text: 'Core Concepts', link: '/en-us/language/' },
                            { text: 'Maintenance', link: '/en-us/maintenance/' },
                        ],
                    },
                    { text: 'FAQ', link: '/en-us/faq' },
                ],
                sidebar: {
                    '/en-us/guide/': [
                        {
                            text: 'Getting Started',
                            items: [
                                { text: 'Overview', link: '/en-us/guide/' },
                                { text: 'Features', link: '/en-us/guide/features' },
                            ],
                        },
                    ],
                    '/en-us/language/': [
                        {
                            text: 'Language Reference',
                            items: [
                                { text: 'Overview', link: '/en-us/language/' },
                                { text: 'Literals', link: '/en-us/language/literals' },
                                { text: 'Control Flow', link: '/en-us/language/control-flow' },
                                { text: 'Definitions', link: '/en-us/language/definitions' },
                                {
                                    text: 'Type System',
                                    link: '/en-us/language/type-system/',
                                    items: [
                                        { text: 'Basic Types', link: '/en-us/language/type-system/' },
                                        { text: 'HKT', link: '/en-us/language/type-system/hkt' },
                                        { text: 'Type Functions', link: '/en-us/language/type-system/type-function' },
                                    ],
                                },
                                {
                                    text: 'Object-Oriented',
                                    link: '/en-us/language/object-oriented/',
                                    items: [
                                        { text: 'Object Model', link: '/en-us/language/object-oriented/' },
                                        {
                                            text: 'Anonymous Classes',
                                            link: '/en-us/language/object-oriented/anonymous-classes',
                                        },
                                        { text: 'Value Classes', link: '/en-us/language/object-oriented/value-class' },
                                        { text: 'Property System', link: '/en-us/language/object-oriented/property' },
                                        { text: 'Inheritance', link: '/en-us/language/object-oriented/inheritance' },
                                        { text: 'Trait System', link: '/en-us/language/object-oriented/trait-system' },
                                        { text: 'Events', link: '/en-us/language/object-oriented/events' },
                                        { text: 'Neural Networks', link: '/en-us/language/object-oriented/neural' },
                                        { text: 'Widget System', link: '/en-us/language/object-oriented/widget' },
                                    ],
                                },
                                {
                                    text: 'Function-Oriented',
                                    link: '/en-us/language/function-oriented/',
                                    items: [
                                        { text: 'Overview', link: '/en-us/language/function-oriented/' },
                                        {
                                            text: 'Anonymous Functions',
                                            link: '/en-us/language/function-oriented/anonymous-functions',
                                        },
                                        {
                                            text: 'Pattern Matching',
                                            link: '/en-us/language/function-oriented/pattern-match',
                                        },
                                    ],
                                },
                                {
                                    text: 'Effect System',
                                    link: '/en-us/language/effect-system/',
                                    items: [
                                        { text: 'Effect System', link: '/en-us/language/effect-system/' },
                                        { text: 'Error Handling', link: '/en-us/language/effect-system/error-handler' },
                                        { text: 'Generators', link: '/en-us/language/effect-system/generator' },
                                        { text: 'Coroutines', link: '/en-us/language/effect-system/coroutine' },
                                        { text: 'AOP', link: '/en-us/language/effect-system/aop' },
                                        { text: 'IoC', link: '/en-us/language/effect-system/ioc' },
                                    ],
                                },
                                {
                                    text: 'Metaprogramming',
                                    link: '/en-us/language/meta-programming/',
                                    items: [
                                        { text: 'Overview', link: '/en-us/language/meta-programming/' },
                                        { text: 'Macros', link: '/en-us/language/meta-programming/macro' },
                                        {
                                            text: 'Multiple Inheritance',
                                            link: '/en-us/language/meta-programming/inheritance',
                                        },
                                    ],
                                },
                                { text: 'Module System', link: '/en-us/language/modules' },
                                { text: 'Trait System', link: '/en-us/language/trait-system' },
                            ],
                        },
                    ],
                    '/en-us/maintenance/': [
                        {
                            text: 'Maintenance Guide',
                            items: [
                                { text: 'Overview', link: '/en-us/maintenance/' },
                                { text: 'Project Architecture', link: '/en-us/maintenance/project-architecture' },
                                { text: 'Salsa Incremental Compilation', link: '/en-us/maintenance/salsa-incremental' },
                                { text: 'Miette Error Handling', link: '/en-us/maintenance/miette-error-handling' },
                                { text: 'Execution Models', link: '/en-us/maintenance/execution-models' },
                            ],
                        },
                    ],
                    '/en-us/examples/': [
                        {
                            text: 'Examples',
                            items: [
                                { text: 'Overview', link: '/en-us/examples/' },
                                {
                                    text: 'Web Development',
                                    link: '/en-us/examples/web-development/',
                                    items: [
                                        { text: 'Overview', link: '/en-us/examples/web-development/' },
                                        { text: 'X-Grammar (XML)', link: '/en-us/examples/web-development/x-grammar' },
                                        {
                                            text: 'V-Grammar (Native)',
                                            link: '/en-us/examples/web-development/v-grammar',
                                        },
                                        { text: 'Widgets', link: '/en-us/examples/web-development/widget' },
                                        { text: 'Events', link: '/en-us/examples/web-development/events' },
                                    ],
                                },
                                {
                                    text: 'Game Development',
                                    link: '/en-us/examples/game-development/',
                                    items: [
                                        { text: 'Overview', link: '/en-us/examples/game-development/' },
                                        { text: 'ECS', link: '/en-us/examples/game-development/ecs' },
                                        { text: 'Shaders', link: '/en-us/examples/game-development/graphics-shader' },
                                        { text: 'GPU Compute', link: '/en-us/examples/game-development/gpu-compute' },
                                    ],
                                },
                                {
                                    text: 'Embedded',
                                    link: '/en-us/examples/embedded-development/',
                                    items: [
                                        { text: 'Overview', link: '/en-us/examples/embedded-development/' },
                                        {
                                            text: 'Digital Circuits',
                                            link: '/en-us/examples/embedded-development/digital-circuits',
                                        },
                                    ],
                                },
                            ],
                        },
                    ],
                },
            },
        },
    },

    vite: {
        optimizeDeps: {
            include: ['dayjs', 'mermaid'],
        },
    },

    markdown: {
        theme: {
            light: 'one-light',
            dark: 'one-dark-pro',
        },
        shikiSetup(shiki) {
            shiki.loadLanguageSync({
                name: 'valkyrie',
                scopeName: 'source.valkyrie',
                fileTypes: ['valkyrie'],
                patterns: valkyrieGrammar.patterns,
                repository: valkyrieGrammar.repository,
            });
        },
    },
    themeConfig: {
        socialLinks: [{ icon: 'github', link: 'https://github.com/valkyrie-language/valkyrie' }],

        footer: {
            message: 'Released under the MIT License.',
            copyright: 'Copyright © 2024 Valkyrie Team',
        },
    },
});

export default withMermaid({
    ...config,
    mermaid: {
        // refer https://mermaid.js.org/config/setup/modules/mermaidAPI.html#mermaidapi-configuration-defaults for options
    },
    // optionally set additional config for plugin itself with MermaidPluginConfig
    mermaidPlugin: {
        class: 'mermaid my-class', // set additional css classes for parent container
    },
});
