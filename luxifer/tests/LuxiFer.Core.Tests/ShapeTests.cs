using LuxiFer.Core.Canvas;

namespace LuxiFer.Core.Tests;

public class ShapeTests
{
    [Fact]
    public void Polyline_Bounds_umfasst_alle_Punkte()
    {
        var poly = new PolylineObject { Points = { (10, 20), (50, 5), (30, 40) } };
        var (x, y, w, h) = poly.Bounds;
        Assert.Equal(10, x);
        Assert.Equal(5, y);
        Assert.Equal(40, w);
        Assert.Equal(35, h);
    }

    [Fact]
    public void Polyline_SetBounds_skaliert_Punkte_proportional()
    {
        var poly = new PolylineObject { Points = { (0, 0), (10, 10) } };
        poly.SetBounds(5, 5, 20, 20);
        Assert.Equal((5, 5), poly.Points[0]);
        Assert.Equal((25, 25), poly.Points[1]);
    }

    [Fact]
    public void Line_MoveBy_verschiebt_beide_Endpunkte()
    {
        var line = new LineObject { X = 0, Y = 0, X2 = 10, Y2 = 10 };
        line.MoveBy(5, -5);
        Assert.Equal((5, -5, 15, 5), (line.X, line.Y, line.X2, line.Y2));
    }

    [Fact]
    public void SetBounds_erzwingt_Mindestgroesse()
    {
        var rect = new RectangleObject { X = 0, Y = 0, Width = 10, Height = 10 };
        rect.SetBounds(0, 0, 0, 0);
        Assert.Equal(0.1, rect.Width);
        Assert.Equal(0.1, rect.Height);
    }

    [Fact]
    public void HitTest_ueberspringt_gesperrte_Layer()
    {
        var doc = new CanvasDocument();
        var layer = Layer.CreateNext(0);
        layer.Locked = true;
        layer.Objects.Add(new RectangleObject { X = 0, Y = 0, Width = 10, Height = 10 });
        doc.Layers.Add(layer);
        Assert.Null(doc.HitTest(5, 5));
        layer.Locked = false;
        Assert.NotNull(doc.HitTest(5, 5));
    }

    [Fact]
    public void CreateNext_vergibt_Farben_reihum()
    {
        Assert.Equal(Layer.SwatchColors[0], Layer.CreateNext(0).ColorHex);
        Assert.Equal(Layer.SwatchColors[1], Layer.CreateNext(1).ColorHex);
        Assert.Equal(Layer.SwatchColors[0], Layer.CreateNext(Layer.SwatchColors.Length).ColorHex);
    }

    [Fact]
    public void Geschlossene_Formen_sind_fuellbar_offene_nicht()
    {
        Assert.True(new RectangleObject().IsFillable);
        Assert.True(new EllipseObject().IsFillable);
        Assert.False(new LineObject().IsFillable);
        Assert.False(new PolylineObject { Closed = false }.IsFillable);
        Assert.True(new PolylineObject { Closed = true }.IsFillable);
    }

    [Fact]
    public void Nur_Fill_und_Raster_Modus_werden_gefuellt()
    {
        Assert.False(LayerMode.Cut.IsFilled());
        Assert.True(LayerMode.Fill.IsFilled());
        Assert.True(LayerMode.Raster.IsFilled());
    }
}
