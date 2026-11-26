#! python 3
"""Optional audit helper for a licensed Rhino installation with GH1 already open.

Run in Rhino's Python 3 ScriptEditor, not inside NODES. No Rhino or Grasshopper
dependency is added to NODES. No component is instantiated or solved by default.
Only use a clean trusted installation. No source, icons, descriptions, local
paths or user-document data are copied. Library classification requires review.
"""
import json
import clr
import Rhino
import rhinoscriptsyntax as rs

clr.AddReference("Grasshopper")
import Grasshopper

# After reviewing the proxy metadata export, set this to the exact GUIDs
# of trusted standard libraries, never third-party or unknown libraries.
APPROVED_STANDARD_LIBRARY_GUIDS = set()
# Optional: instantiate explicitly selected trusted component IDs for port metadata.
INSPECT_COMPONENT_GUIDS = set()


def port(p):
    # Persistent defaults vary by type and build; do not confuse volatile data
    # or absence of a stored value with a documented default.
    return {
        "name": str(p.Name),
        "nickname": str(p.NickName),
        "type": str(p.TypeName),
        "access": str(p.Access),
        "optional": bool(p.Optional),
        "data_mapping": str(p.DataMapping),
        "default": None,
    }


def main():
    server = Grasshopper.Instances.ComponentServer
    if server is None or server.IsLoading:
        raise RuntimeError("Open GH1 and wait until its component libraries are loaded.")
    version = str(Rhino.RhinoApp.Version)
    assembly_version = str(server.GetType().Assembly.GetName().Version)
    rows = []
    errors = []
    approved = {x.lower() for x in APPROVED_STANDARD_LIBRARY_GUIDS}
    inspect = {x.lower() for x in INSPECT_COMPONENT_GUIDS}
    for proxy in server.ObjectProxies:
        guid = str(proxy.Guid).lower()
        try:
            library = str(proxy.LibraryGuid).lower()
            obsolete = bool(proxy.Obsolete)
            kind = str(proxy.Kind)
            row = {
                "version_family": "GH1",
                "rhino_build": version,
                "gh_assembly_version": assembly_version,
                "component_guid": guid,
                "library_guid": library,
                "name": str(proxy.Desc.Name),
                "category": str(proxy.Desc.Category),
                "subcategory": str(proxy.Desc.SubCategory),
                "proxy_kind": kind,
                "exposure": str(proxy.Exposure),
                "obsolete": obsolete,
                "classification": (
                    "OBSOLETE" if obsolete else
                    "REVIEWED_LIBRARY_CANDIDATE" if library in approved else
                    "UNCLASSIFIED"
                ),
                "inputs": None,
                "outputs": None,
                "defaults": None,
                "tree_behavior": None,
                "status": "UNKNOWN",
                "test_status": "GH_REFERENCE_NOT_RUN",
            }
            # Even trusted assemblies may have hidden/experimental components.
            # Do not execute or instantiate unreviewed library objects.
            if guid in inspect and library in approved and not obsolete and proxy.Type is not None:
                try:
                    obj = server.EmitObject(proxy.Guid)
                    if obj is not None and hasattr(obj, "Params"):
                        row["inputs"] = [port(p) for p in obj.Params.Input]
                        row["outputs"] = [port(p) for p in obj.Params.Output]
                    elif obj is not None and hasattr(obj, "Access"):
                        row["parameter"] = port(obj)
                except Exception as exc:
                    row["port_inspection_error"] = type(exc).__name__
            rows.append(row)
        except Exception as exc:
            errors.append({"component_guid": guid, "error_type": type(exc).__name__})
    rows.sort(key=lambda r: (r["category"], r["subcategory"], r["name"], r["component_guid"]))
    data = {
        "schema_version": 1,
        "version_family": "GH1",
        "rhino_build": version,
        "gh_assembly_version": assembly_version,
        "proxy_count": len(rows),
        "error_count": len(errors),
        "errors": errors,
        "components": rows,
        "note": "Loaded proxy count is not the standard non-obsolete component count. Review library, exposure, proxy type, obsolete and experimental status before inclusion."
    }
    path = rs.SaveFileName("Save GH1 proxy metadata (no descriptions/icons)", "JSON (*.json)|*.json||")
    if not path:
        return
    # Do not silently overwrite another audit's snapshot.
    with open(path, "x", encoding="utf-8") as out:
        json.dump(data, out, ensure_ascii=False, indent=2)
        out.write("\n")
    print("Exported {} proxies with {} errors; classification pending.".format(len(rows), len(errors)))


if __name__ == "__main__":
    main()
