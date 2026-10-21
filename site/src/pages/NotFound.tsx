import { Static } from "@ejm/ssg";

const NotFoundContent =
    import.meta.env.SSR || import.meta.env.DEV
        ? (await import("./not-found.server")).default
        : undefined;

export default function NotFound() {
    return (
        <Static
            className="contents"
            render={NotFoundContent ? () => <NotFoundContent /> : undefined}
        />
    );
}
