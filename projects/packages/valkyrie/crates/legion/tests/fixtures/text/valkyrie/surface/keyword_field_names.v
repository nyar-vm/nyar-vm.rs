
structure LegionPublishTarget {
    target: utf8
    type: utf8
    package_id: utf8
}

micro empty_publish_target() -> LegionPublishTarget {
    return LegionPublishTarget {
        target: "",
        type: "",
        package_id: ""
    }
}

micro publish_type_of(target: LegionPublishTarget) -> utf8 {
    return target.type
}

