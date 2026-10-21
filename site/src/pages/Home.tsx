import { Hero } from "@ejm/ui/layout/Hero";
import { ShowcaseFrame } from "@ejm/demos/ShowcaseFrame";
import { LazyHarnessPreview } from "../demo";
import { Static } from "@ejm/ssg";

// Copy stays in the built HTML. Playback remains a live shared island outside Static.
const content =
    import.meta.env.SSR || import.meta.env.DEV ? await import("../home-parts") : undefined;

export default function Home() {
    return (
        <>
            <Hero
                eyebrow="ejm-harness / open source / MIT"
                title="Rust engine for coding agents."
                actions={
                    content ? (
                        <Static className="contents" render={() => <content.HeroActions />} />
                    ) : (
                        <Static className="contents" />
                    )
                }
                aside={
                    <ShowcaseFrame
                        id="demo"
                        label="ejm-harness example"
                        note="Formatted local recording. No worker or model runs here. Resource figures are admission reservations."
                    >
                        <LazyHarnessPreview />
                    </ShowcaseFrame>
                }
            >
                {content ? (
                    <Static className="contents" render={() => <content.HeroIntro />} />
                ) : (
                    <Static className="contents" />
                )}
            </Hero>
            {content ? (
                <Static
                    className="contents"
                    render={() => (
                        <>
                            <content.How />
                            <content.Rest />
                        </>
                    )}
                />
            ) : (
                <Static className="contents" />
            )}
        </>
    );
}
